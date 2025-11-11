This folder holds capability files for Tauri v2.

default.json is used to grant fine-grained permissions to the `main` webview/window.

Notes:
- JSON does not support comments. To document why a permission exists, use this README.
- We added explicit event permissions so the UI can call `window.__TAURI__.event.listen` and `window.__TAURI__.event.emit` safely.

If you later change capabilities, update this README with the reason and any security considerations.
