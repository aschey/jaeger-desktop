if (!document.extraScriptsLoaded) {
  document.extraScriptsLoaded = true;
  (async () => {
    let s = document.createElement("script");
    s.type = "text/javascript";
    let resourceDir = await window.__TAURI__.path.resourceDir();
    let path = await window.__TAURI__.path.join(
      resourceDir,
      "resources/darkreader.min.js",
    );
    s.src = window.__TAURI__.core.convertFileSrc(path);
    s.onload = function () {
      DarkReader.enable({});
    };
    document.head.appendChild(s);
  })().catch((err) => {
    console.error(err);
  });

  const observeChanges = () => {
    const body = document.querySelector("body");
    const observer = new MutationObserver((mutations) => {
      for (const node of document.getElementsByTagName("a")) {
        if (node && node.getAttribute("href")) {
          node.setAttribute("target", "_self");
        }
      }
    });
    observer.observe(body, { childList: true, subtree: true });
  };

  window.onload = observeChanges;
}
