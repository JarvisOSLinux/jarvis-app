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
├── resources/
│   └── wake_chime.wav  Bundled default wake-word sound (one-time override)
├── Cargo.toml
├── tauri.conf.json   Window config, withGlobalTauri: true, bundle.resources
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
                      `stop_stream`, `confirmation_response`, `ping`,
                      `list_sessions`, `create_session`, `switch_session`,
                      `rename_session`, `delete_session`,
                      `set_wake_chime_path`, `reset_wake_chime_path`,
                      `get_settings`, `set_confirmation_mode`,
                      `list_providers`, `add_provider`, `edit_provider`,
                      `remove_provider`
**Daemon -> Client**: `state`, `response` (streaming), `wake_word_detected`,
                      `confirmation_request`, `error`, `session_list`,
                      `session_switched`, `session_error`,
                      `config_updated`, `config_error`, `settings`,
                      `provider_list`, `provider_error`

Session CRUD (`Project-JARVIS`'s `jarvis/runtime/io.py`) is a thin wrapper
over the existing `SessionManager` (`new_session`/`list`/`switch`/`rename`/
`delete`) -- no separate storage on the GUI socket side. `switch_session`
returns the session plus its `conversation_log` history as
`{role, content, timestamp}` messages so a client can render it immediately.
Session mutations (create/switch/rename/delete) broadcast to every connected
GUI client, since there's one shared "current session" pointer on the
daemon; `list_sessions` and errors reply to the requester only.

States: `idle`, `woken`, `capturing`, `listening`, `processing`, `speaking`, `offline`.
`woken`/`capturing`/`processing`/`speaking` come from the daemon's formal
voice/response state machine (`Project-JARVIS`#141); `listening` is the
separate manual mic-toggle state; `offline` is client-side only (never sent
by the daemon). The daemon's `state` message can also carry a `meta` object
(currently just a discard reason) -- not yet consumed here.

### Wake chime one-time bootstrap (Project-JARVIS#139)

On first run only, jarvis-app overrides the daemon's `WAKE_CHIME_PATH` to
its own bundled sound (`resources/wake_chime.wav`, a distinct three-note
arpeggio from `Project-JARVIS`'s own bundled default) — proving an external
project can actually customize the daemon's chime, not just that the config
key theoretically exists. Implementation (`lib.rs`):

- `pending_wake_chime_bootstrap()` checks a marker file in the app's own
  data dir (`app_data_dir()/.wake_chime_bootstrapped`, distinct from the
  daemon's `jarvis_data_dir()` used for socket discovery) at startup. If it
  exists, nothing happens -- this really does run only once, ever.
- If absent, the bundled resource path is resolved and `set_wake_chime_path`
  is sent on every `Connected` event (retried across reconnects) until a
  `config_updated` response for `WAKE_CHIME_PATH` confirms success, at which
  point the marker is written and the poll loop stops sending it.
- A `config_error` response is logged and left to retry on the next
  reconnect/app launch -- a daemon that's merely offline on first launch
  shouldn't silently skip the bootstrap forever.
- After this one-time bootstrap, `jarvisos-app`#12's settings panel is the
  ongoing, user-facing surface for changing the wake sound -- this code
  never touches `WAKE_CHIME_PATH` again.

### Settings panel (Project-JARVIS#12)

A centered modal (`#settings-btn` in the header) with two tabs:

- **General** -- voice-activation toggle (an alias for the same
  `toggle_listening` command the mic button uses), confirmation mode
  (`smart`/`ask_all`/`allow_all`), wake sound (native file picker via
  `tauri-plugin-dialog`'s `pick_wake_chime_file`, or "Restore default" via
  `reset_wake_chime_path`), and a read-only socket path for troubleshooting
  (`get_connection_info` -- local only, never round-trips to the daemon).
- **Providers** -- list/add/edit/remove backed by the daemon's
  `jarvis/core/providers.py` CRUD (shared with the TUI's own config modal).
  **Provider changes only take effect on the daemon's next restart** -- there
  is no live `ProviderPool` reload, matching the TUI's own limitation; the
  panel says so rather than implying an instant effect.

`ConfigUpdated`/`ConfigError` (`ipc.rs`) already existed for the wake-chime
bootstrap above, but were previously consumed only internally by the poll
loop -- never forwarded to the frontend. They're now also emitted as
`ipc-config-updated`/`ipc-config-error` unconditionally, so the settings
panel can reflect a `CONFIRMATION_MODE` or `WAKE_CHIME_PATH` write from
*any* source (including another connected client), not just its own.

### Tauri Command / Event Mapping

| Tauri command (JS → Rust)       | Tauri event (Rust → JS)  |
|---------------------------------|--------------------------|
| `send_message`                  | `ipc-connected`          |
| `toggle_listening`              | `ipc-disconnected`       |
| `stop_stream`                   | `ipc-state` (string)     |
| `send_confirmation_response`    | `ipc-chunk` `{content, done}` |
| `list_confirmations`            | `ipc-wake`               |
| `approve_confirmation`          | `ipc-confirm` `{id, tool_names}` |
| `deny_confirmation`             | `ipc-confirmation-list` `[{id, tool_names, created_at}]` |
| `approve_all_confirmations`     |                          |
| `get_settings`                  | `ipc-settings` `{confirmation_mode, wake_chime_path}` |
| `set_confirmation_mode`         | `ipc-config-updated` `{key, value}` |
| `reset_wake_chime_path`         | `ipc-config-error` `{key, message}` |
| `list_providers` / `add_provider` / `edit_provider` / `remove_provider` | `ipc-provider-list` `[{name, type, model, ...}]` |
| `pick_wake_chime_file` (native file dialog, no daemon round-trip) | `ipc-provider-error` (string) |
| `get_connection_info` (local socket path, no daemon round-trip) | |

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
