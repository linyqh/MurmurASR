import React from "react";
import ReactDOM from "react-dom/client";
import { StoreProvider } from "./lib/store";
import { App } from "./App";
import "./styles/global.css";
import "./styles/views.css";

// Browser preview of the UI without the Tauri runtime (dev only, tree-shaken from builds).
if (import.meta.env.DEV && new URLSearchParams(location.search).has("mock")) {
  const { installMock } = await import("./dev/mockTauri");
  installMock();
}

const ua = navigator.userAgent;
const platform = /Mac/.test(ua) ? "macos" : /Windows/.test(ua) ? "windows" : "linux";
document.documentElement.dataset.platform = platform;

// Native apps don't show a browser context menu outside text fields.
document.addEventListener("contextmenu", (e) => {
  const target = e.target as HTMLElement;
  if (!target.closest("input, textarea, [contenteditable], .selectable")) e.preventDefault();
});

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <StoreProvider>
      <App />
    </StoreProvider>
  </React.StrictMode>,
);
