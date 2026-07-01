# CLAUDE.md — jarvisos-app

## What This Is

Desktop UI for the JARVIS daemon. ChatGPT-style chat interface built with
Rust + Tauri 2 + HTML/CSS/JS. Connects to the JARVIS Python daemon over a
bidirectional Unix socket using newline-delimited JSON.

## Role in the JARVIS Ecosystem

jarvisos-app is the graphical alternative to Project-JARVIS's Textual TUI.
Both connect to the same daemon over the same IPC protocol. The TUI is the
primary workspace; the Tauri app is for desktop Linux users who prefer a GUI.

## Tech Stack

- Rust + Tauri 2 (native backend, WebKitGTK frontend on Linux)
- HTML/CSS/JS (no framework, no bundler)
- serde / serde_json for JSON IPC and Tauri command payloads
- WebKitGTK 4.1 (Linux), WebView2 (Windows), WKWebView (macOS)

## Architecture

```
src-tauri/
├── src/
│   ├── main.rs       Tauri entry point
│   ├── lib.rs        #[tauri::command] handlers + IPC poll thread
│   └── ipc.rs        IPC client with auto-reconnect (background thread)
├── Cargo.toml
├── tauri.conf.json   Window config, withGlobalTauri: true
└── build.rs          tauri-build

src/
├── index.html        App shell (header, chat-view, input-bar)
├── styles.css        Dark navy + cyan theme
└── main.js           IPC event listeners, message rendering

python/
├── ipc_server.py          Async Unix socket server (daemon side)
└── main_integration.py    Integration guide for wiring into JARVIS daemon
```

### IPC Protocol

Socket: `~/.local/share/jarvis/jarvis.sock` (mirrors daemon's data_dir).
Override with `JARVIS_SOCKET` env var. Newline-delimited JSON.

**Client -> Daemon**: `message`, `start_listening`, `stop_listening`,
                      `stop_stream`, `confirmation_response`, `ping`
**Daemon -> Client**: `state`, `response` (streaming), `wake_word_detected`,
                      `confirmation_request`, `error`

States: `idle`, `listening`, `processing`, `speaking`, `offline`

### Tauri Command / Event Mapping

| Tauri command (JS → Rust)       | Tauri event (Rust → JS)  |
|---------------------------------|--------------------------|
| `send_message`                  | `ipc-connected`          |
| `toggle_listening`              | `ipc-disconnected`       |
| `stop_stream`                   | `ipc-state` (string)     |
| `send_confirmation_response`    | `ipc-chunk` `{content, done}` |
|                                 | `ipc-wake`               |
|                                 | `ipc-confirm` `{id, description}` |

## Build

### Prerequisites

Arch/CachyOS: `sudo pacman -S webkit2gtk-4.1 gtk3 rust`
Fedora:       `sudo dnf install webkit2gtk4.1-devel gtk3-devel rust cargo`

Install Tauri CLI: `cargo install tauri-cli`

### Compile

```bash
# Development (hot-reload frontend, native backend)
cargo tauri dev

# Release build
cargo tauri build
# Binary: src-tauri/target/release/jarvis-ui
```

## Theme

Dark navy (`#0a0e1a`) + JARVIS cyan (`#00c8ff`). Monospace font: Hack/JetBrains Mono.

## Conventions

- `cargo fmt` + `cargo clippy` clean before pushing
- Commit messages: imperative mood
- No comments explaining what code does; only non-obvious WHY
