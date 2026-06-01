# CLAUDE.md — jarvisos-app

## What This Is

Desktop UI for the JARVIS daemon. ChatGPT-style chat interface built with
Rust + CXX-Qt 0.7 + Qt6/QML. Connects to the JARVIS Python daemon over a
bidirectional Unix socket using newline-delimited JSON.

Also includes a lightweight floating widget variant (`jarvis-widget/`).

## Role in the JARVIS Ecosystem

jarvisos-app is the graphical alternative to Project-JARVIS's Textual TUI.
Both connect to the same daemon over the same IPC protocol. The TUI is the
primary workspace; the Qt app is for desktop Linux users who prefer a GUI.

## Tech Stack

- Rust + CXX-Qt 0.7 (Rust <-> C++ <-> QML bridge)
- Qt6 (QtBase + QtDeclarative/QML)
- CMake + Ninja (Qt6 build system)
- serde / serde_json for JSON IPC
- chrono for timestamps

## Architecture

```
rust/
├── src/
│   ├── main.rs       Qt application entry point
│   ├── ipc.rs        IPC client with auto-reconnect (background thread)
│   └── bridge.rs     CXX-Qt bridge (QML <-> Rust signals/slots)
├── qml/
│   ├── Main.qml           Root window, header, layout
│   ├── ChatView.qml       Message list with streaming support
│   ├── MessageBubble.qml  User/JARVIS message bubbles
│   ├── InputBar.qml       Text input + mic + send button
│   └── StatusIndicator.qml Animated status dot
├── build.rs          CXX-Qt compilation
└── resources.qrc     Qt resource bundle

python/
├── ipc_server.py          Async Unix socket server (daemon side)
└── main_integration.py    Integration guide for wiring into JARVIS daemon

jarvis-widget/             Lightweight floating widget (same tech stack, simpler UI)
```

### IPC Protocol

Socket: `/tmp/jarvis.sock`, newline-delimited JSON.

**Client -> Daemon**: `message`, `start_listening`, `stop_listening`, `ping`
**Daemon -> Client**: `state`, `response` (streaming), `wake_word_detected`, `error`

States: `idle`, `listening`, `processing`, `speaking`, `offline`

## Build

### Prerequisites

Arch: `sudo pacman -S qt6-base qt6-declarative cmake ninja rust`
Fedora: `sudo dnf install qt6-qtbase-devel qt6-qtdeclarative-devel cmake ninja-build`

### Compile

```bash
cd rust
cargo build --release
# Binary: rust/target/release/jarvis-ui
```

## Theme

Dark navy (#0a0e1a) + JARVIS cyan (#00c8ff). Monospace font: Hack/JetBrains Mono.

## Conventions

- `cargo fmt` + `cargo clippy` clean before pushing
- Commit messages: imperative mood
- No comments explaining what code does; only non-obvious WHY
