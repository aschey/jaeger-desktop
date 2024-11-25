use std::{
    fs, io,
    sync::{Arc, Mutex},
    time::Duration,
};

use tauri::{
    menu::{Menu, MenuBuilder, MenuItem, MenuItemBuilder},
    path::BaseDirectory,
    window::Color,
    Manager, Url, WebviewUrl, WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_shell::{
    process::{CommandChild, CommandEvent},
    ShellExt,
};
use tokio::sync::Notify;

struct AppData {
    child: Mutex<Option<CommandChild>>,
    shutdown: Arc<Notify>,
}

pub fn run() {
    let port = 16686;
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(move |app| {
            let config_path = app
                .path()
                .resolve("resources/config.yml", BaseDirectory::Resource)
                .unwrap();
            let config_ui_path = app
                .path()
                .resolve("resources/config-ui.json", BaseDirectory::Resource)
                .unwrap();
            let prometheus_config_path = app
                .path()
                .resolve("resources/prometheus.yml", BaseDirectory::Resource)
                .unwrap();
            let config_dir = dirs::config_dir().unwrap().join("jaeger");
            fs::create_dir_all(&config_dir).unwrap();
            let user_config_path = config_dir.join("config.yml");
            let user_ui_config_path = config_dir.join("config-ui.json");
            let user_prometheus_config_path = config_dir.join("prometheus.yml");

            if !user_config_path.exists() {
                let content = fs::read_to_string(config_path).unwrap();
                let content = content.replacen(
                    "{{UI_CONFIG_FILE}}",
                    &user_ui_config_path.to_string_lossy(),
                    1,
                );
                fs::write(&user_config_path, content).unwrap();
            }
            if !user_ui_config_path.exists() {
                fs::copy(config_ui_path, &user_ui_config_path).unwrap();
            }
            if !user_prometheus_config_path.exists() {
                fs::copy(prometheus_config_path, &user_prometheus_config_path).unwrap();
            }

            let prometheus_command = app.shell().sidecar("prometheus").unwrap().args([
                "--config.file",
                user_prometheus_config_path
                    .to_string_lossy()
                    .to_string()
                    .as_str(),
                "--storage.tsdb.path",
                "/tmp/prometheus",
                "--web.enable-otlp-receiver",
            ]);
            let (mut rx, child) = prometheus_command.spawn().expect("failed to spawn");
            tauri::async_runtime::spawn(async move {
                while let Some(event) = rx.recv().await {
                    match event {
                        CommandEvent::Stdout(bytes) => {
                            let line = String::from_utf8_lossy(&bytes);
                            println!("prometheus stdout: {line}");
                        }
                        CommandEvent::Stderr(bytes) => {
                            let line = String::from_utf8_lossy(&bytes);
                            println!("prometheus stderr: {line}");
                        }
                        CommandEvent::Terminated(payload) => {
                            println!("terminated {payload:?}");
                        }
                        _ => {
                            println!("{event:?}");
                        }
                    }
                }
            });

            let jaeger_command = app.shell().sidecar("jaeger").unwrap().args([
                "--config",
                user_config_path.to_string_lossy().to_string().as_str(),
            ]);

            let (mut rx, child) = jaeger_command.spawn().expect("Failed to spawn sidecar");
            let shutdown = Arc::new(Notify::new());
            app.manage(AppData {
                child: Mutex::new(Some(child)),
                shutdown: shutdown.clone(),
            });

            tauri::async_runtime::spawn(async move {
                while let Some(event) = rx.recv().await {
                    match event {
                        CommandEvent::Stdout(bytes) => {
                            let line = String::from_utf8_lossy(&bytes);
                            println!("stdout: {line}");
                        }
                        CommandEvent::Stderr(bytes) => {
                            let line = String::from_utf8_lossy(&bytes);
                            println!("stderr: {line}");
                        }
                        CommandEvent::Terminated(payload) => {
                            println!("terminated {payload:?}");
                            shutdown.notify_one();
                        }
                        _ => {
                            println!("{event:?}");
                        }
                    }
                }
            });
            let url: Url = format!("http://localhost:{}", port).parse().unwrap();

            let client = reqwest::blocking::Client::new();
            loop {
                if let Ok(res) = client.head(url.clone()).send() {
                    if res.status().is_success() {
                        break;
                    }
                    std::thread::sleep(Duration::from_secs(1));
                }
            }
            let script_path = app
                .path()
                .resolve("resources/inject.js", BaseDirectory::Resource)
                .unwrap();
            let script = fs::read_to_string(script_path).unwrap();
            let handle = app.handle();
            WebviewWindowBuilder::new(app, "main".to_string(), WebviewUrl::External(url))
                .title("Localhost Example")
                .background_color(Color(0, 0, 0, 255))
                .on_page_load(move |window, _payload| {
                    window.eval(&script).unwrap();
                })
                .build()?;
            let back = MenuItemBuilder::new("Back").id("back").build(handle)?;
            let forward = MenuItemBuilder::new("Forward")
                .id("forward")
                .build(handle)?;

            app.set_menu(
                MenuBuilder::new(handle)
                    .item(&back)
                    .item(&forward)
                    .build()?,
            )?;

            Ok(())
        })
        .on_menu_event(|app, event| {
            let windows = app.webview_windows();
            let window = windows.get("main").unwrap();
            if event.id() == "back" {
                window.eval("history.back()").unwrap();
            }
            if event.id() == "forward" {
                window.eval("history.forward()").unwrap();
            }
        })
        .on_window_event(move |window, event| {
            if let WindowEvent::CloseRequested { .. } = event {
                let data = window.state::<AppData>();
                let child = data.child.lock().unwrap().take().unwrap();
                #[cfg(unix)]
                {
                    if unsafe { libc::kill(child.pid() as i32, libc::SIGTERM) } == -1 {
                        println!("{:?}", io::Error::last_os_error());
                    }

                    if tauri::async_runtime::block_on(async {
                        tokio::time::timeout(Duration::from_secs(2), data.shutdown.notified()).await
                    })
                    .is_ok()
                    {
                        return;
                    }
                }
                child.kill().unwrap();
                tauri::async_runtime::block_on(async {
                    data.shutdown.notified().await;
                });
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
