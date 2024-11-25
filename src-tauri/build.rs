use std::{fs, io::Read};

fn main() {
    let darkreader_path = "./resources/darkreader.min.js";
    println!("cargo::rerun-if-changed={darkreader_path}");

    if !fs::exists(darkreader_path).unwrap() {
        let darkreader = reqwest::blocking::get(
            "https://cdn.jsdelivr.net/npm/darkreader@4.9.96/darkreader.min.js",
        )
        .unwrap();
        if !darkreader.status().is_success() {
            panic!("failed to fetch dark reader: {:?}", darkreader.status());
        }
        fs::write(darkreader_path, darkreader.bytes().unwrap()).unwrap();
    }
    tauri_build::build()
}
