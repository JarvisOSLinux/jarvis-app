mod ipc;

use ipc::{IpcClient, IpcEvent};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::menu::{CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder};
use tauri::path::BaseDirectory;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tauri_plugin_autostart::ManagerExt;

const TRAY_ID: &str = "main-tray";
const MAIN_WINDOW: &str = "main";
const WAKE_CHIME_MARKER_FILE: &str = ".wake_chime_bootstrapped";
const WAKE_CHIME_RESOURCE: &str = "resources/wake_chime.wav";

/// Resolves (bundled chime path, marker path) if the one-time
/// WAKE_CHIME_PATH override (Project-JARVIS#139's "proof of modularity"
/// criterion) hasn't happened yet. None if it already has, or if the
/// bundled resource/app data dir can't be resolved.
fn pending_wake_chime_bootstrap(app: &AppHandle) -> Option<(String, PathBuf)> {
    let marker_path = app.path().app_data_dir().ok()?.join(WAKE_CHIME_MARKER_FILE);
    let resource_path = app
        .path()
        .resolve(WAKE_CHIME_RESOURCE, BaseDirectory::Resource)
        .ok()?;
    wake_chime_bootstrap_decision(marker_path, resource_path)
}

/// Pure decision logic split out from `pending_wake_chime_bootstrap` so it's
/// testable without a running Tauri app / AppHandle.
fn wake_chime_bootstrap_decision(
    marker_path: PathBuf,
    resource_path: PathBuf,
) -> Option<(String, PathBuf)> {
    if marker_path.exists() {
        return None;
    }
    if !resource_path.is_file() {
        return None;
    }
    Some((resource_path.to_string_lossy().into_owned(), marker_path))
}

struct AppState {
    ipc: Arc<Mutex<IpcClient>>,
    is_listening: Arc<AtomicBool>,
    connected: Arc<AtomicBool>,
    daemon_state: Arc<Mutex<String>>,
}

#[derive(Serialize, Clone)]
struct ChunkPayload {
    content: String,
    done: bool,
}

#[derive(Serialize, Clone)]
struct ConfirmPayload {
    id: String,
    tool_names: Vec<String>,
}

#[derive(Serialize, Clone)]
struct StatusPayload {
    connected: bool,
    state: String,
}

#[derive(Serialize, Clone)]
struct SessionSwitchedPayload {
    session: serde_json::Value,
    messages: Vec<serde_json::Value>,
}

#[derive(Serialize, Clone)]
struct SettingsPayload {
    confirmation_mode: String,
    wake_chime_path: String,
}

#[derive(Serialize, Clone)]
struct ConfigUpdatedPayload {
    key: String,
    value: String,
}

#[derive(Serialize, Clone)]
struct ConfigErrorPayload {
    key: String,
    message: String,
}

#[tauri::command]
fn send_message(content: String, state: State<AppState>) {
    if content.trim().is_empty() {
        return;
    }
    state.ipc.lock().unwrap().send_message(content);
}

#[tauri::command]
fn toggle_listening(state: State<AppState>, app: AppHandle) {
    let was_listening = state.is_listening.load(Ordering::Relaxed);
    if was_listening {
        state.ipc.lock().unwrap().stop_listening();
        state.is_listening.store(false, Ordering::Relaxed);
        let _ = app.emit("ipc-state", "idle");
    } else {
        state.ipc.lock().unwrap().start_listening();
        state.is_listening.store(true, Ordering::Relaxed);
        let _ = app.emit("ipc-state", "listening");
    }
}

#[tauri::command]
fn stop_stream(state: State<AppState>) {
    state.ipc.lock().unwrap().stop_stream();
}

#[tauri::command]
fn send_confirmation_response(id: String, approved: bool, state: State<AppState>) {
    state
        .ipc
        .lock()
        .unwrap()
        .confirmation_response(id, approved);
}

#[tauri::command]
fn list_confirmations(state: State<AppState>) {
    state.ipc.lock().unwrap().list_confirmations();
}

#[tauri::command]
fn approve_confirmation(id: String, state: State<AppState>) {
    state.ipc.lock().unwrap().approve_confirmation(id);
}

#[tauri::command]
fn deny_confirmation(id: String, state: State<AppState>) {
    state.ipc.lock().unwrap().deny_confirmation(id);
}

#[tauri::command]
fn approve_all_confirmations(state: State<AppState>) {
    state.ipc.lock().unwrap().approve_all_confirmations();
}

#[tauri::command]
fn list_sessions(state: State<AppState>) {
    state.ipc.lock().unwrap().list_sessions();
}

#[tauri::command]
fn create_session(title: Option<String>, state: State<AppState>) {
    state.ipc.lock().unwrap().create_session(title);
}

#[tauri::command]
fn switch_session(id: String, state: State<AppState>) {
    state.ipc.lock().unwrap().switch_session(id);
}

#[tauri::command]
fn rename_session(id: String, title: String, state: State<AppState>) {
    state.ipc.lock().unwrap().rename_session(id, title);
}

#[tauri::command]
fn delete_session(id: String, state: State<AppState>) {
    state.ipc.lock().unwrap().delete_session(id);
}

#[tauri::command]
fn get_settings(state: State<AppState>) {
    state.ipc.lock().unwrap().get_settings();
}

// Read-only troubleshooting info for the settings panel's General tab --
// the socket path itself, not a live setting, so it doesn't round-trip
// through the daemon.
#[tauri::command]
fn get_connection_info() -> String {
    ipc::socket_path()
}

#[tauri::command]
fn set_confirmation_mode(mode: String, state: State<AppState>) {
    state.ipc.lock().unwrap().set_confirmation_mode(mode);
}

#[tauri::command]
fn reset_wake_chime_path(state: State<AppState>) {
    state.ipc.lock().unwrap().reset_wake_chime_path();
}

#[tauri::command]
fn list_providers(state: State<AppState>) {
    state.ipc.lock().unwrap().list_providers();
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn add_provider(
    ptype: String,
    model: String,
    name: Option<String>,
    url: Option<String>,
    api_key: Option<String>,
    temperature: Option<f64>,
    state: State<AppState>,
) {
    state
        .ipc
        .lock()
        .unwrap()
        .add_provider(ptype, model, name, url, api_key, temperature);
}

#[tauri::command]
fn edit_provider(name: String, fields: serde_json::Value, state: State<AppState>) {
    state.ipc.lock().unwrap().edit_provider(name, fields);
}

#[tauri::command]
fn remove_provider(name: String, state: State<AppState>) {
    state.ipc.lock().unwrap().remove_provider(name);
}

#[tauri::command]
async fn pick_wake_chime_file(app: AppHandle) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = std::sync::mpsc::channel();
    app.dialog()
        .file()
        .add_filter("WAV audio", &["wav"])
        .pick_file(move |picked| {
            let _ = tx.send(picked);
        });
    rx.recv().ok().flatten().map(|p| p.to_string())
}

// Frontend pulls this once on load to reconcile state in case ipc-connected /
// ipc-state fired (from the backend's IPC poll thread, started in setup())
// before the webview finished loading and registering its event listeners --
// Tauri does not queue events for listeners that attach after emission.
#[tauri::command]
fn get_status(state: State<AppState>) -> StatusPayload {
    StatusPayload {
        connected: state.connected.load(Ordering::Relaxed),
        state: state.daemon_state.lock().unwrap().clone(),
    }
}

fn set_tray_tooltip(app: &AppHandle, tooltip: &str) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_tooltip(Some(tooltip));
    }
}

fn toggle_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
    } else {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let ipc = Arc::new(Mutex::new(IpcClient::new()));
    let ipc_poll = ipc.clone();
    let is_listening = Arc::new(AtomicBool::new(false));
    let is_listening_poll = is_listening.clone();
    let connected = Arc::new(AtomicBool::new(false));
    let connected_poll = connected.clone();
    let daemon_state = Arc::new(Mutex::new("offline".to_string()));
    let daemon_state_poll = daemon_state.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            ipc,
            is_listening,
            connected,
            daemon_state,
        })
        .setup(|app| {
            let handle = app.handle().clone();
            let wake_chime_bootstrap = pending_wake_chime_bootstrap(&handle);
            std::thread::Builder::new()
                .name("jarvis-ipc-poll".into())
                .spawn(move || {
                    poll_loop(
                        ipc_poll,
                        is_listening_poll,
                        connected_poll,
                        daemon_state_poll,
                        handle,
                        wake_chime_bootstrap,
                    )
                })
                .expect("failed to spawn IPC poll thread");

            let show_hide = MenuItemBuilder::with_id("show_hide", "Show/Hide JARVIS").build(app)?;
            let autostart_enabled = app.autolaunch().is_enabled().unwrap_or(false);
            let start_on_login = CheckMenuItemBuilder::with_id("start_on_login", "Start on Login")
                .checked(autostart_enabled)
                .build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
            let tray_menu = MenuBuilder::new(app)
                .item(&show_hide)
                .item(&start_on_login)
                .separator()
                .item(&quit)
                .build()?;

            let mut tray_builder = TrayIconBuilder::with_id(TRAY_ID)
                .menu(&tray_menu)
                .tooltip("JARVIS -- Offline")
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show_hide" => toggle_main_window(app),
                    "start_on_login" => {
                        let mgr = app.autolaunch();
                        let enabled = mgr.is_enabled().unwrap_or(false);
                        let _ = if enabled { mgr.disable() } else { mgr.enable() };
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        toggle_main_window(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon().cloned() {
                tray_builder = tray_builder.icon(icon);
            }
            tray_builder.build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != MAIN_WINDOW {
                return;
            }
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            send_message,
            toggle_listening,
            stop_stream,
            send_confirmation_response,
            get_status,
            list_confirmations,
            approve_confirmation,
            deny_confirmation,
            approve_all_confirmations,
            list_sessions,
            create_session,
            switch_session,
            rename_session,
            delete_session,
            get_settings,
            get_connection_info,
            set_confirmation_mode,
            reset_wake_chime_path,
            list_providers,
            add_provider,
            edit_provider,
            remove_provider,
            pick_wake_chime_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Tauri application");
}

fn poll_loop(
    ipc: Arc<Mutex<IpcClient>>,
    is_listening: Arc<AtomicBool>,
    connected: Arc<AtomicBool>,
    daemon_state: Arc<Mutex<String>>,
    app: AppHandle,
    mut wake_chime_bootstrap: Option<(String, PathBuf)>,
) {
    let set_state = |s: &str| *daemon_state.lock().unwrap() = s.to_string();

    loop {
        loop {
            let event = ipc.lock().unwrap().try_recv();
            let Some(ev) = event else { break };
            match ev {
                IpcEvent::Connected => {
                    connected.store(true, Ordering::Relaxed);
                    set_state("idle");
                    set_tray_tooltip(&app, "JARVIS -- Connected");
                    let _ = app.emit("ipc-connected", ());
                    let _ = app.emit("ipc-state", "idle");
                    // Retried on every (re)connect until config_updated
                    // confirms it -- see the ConfigUpdated/ConfigError arms.
                    if let Some((path, _)) = &wake_chime_bootstrap {
                        ipc.lock().unwrap().set_wake_chime_path(path.clone());
                    }
                }
                IpcEvent::Disconnected => {
                    connected.store(false, Ordering::Relaxed);
                    is_listening.store(false, Ordering::Relaxed);
                    set_state("offline");
                    set_tray_tooltip(&app, "JARVIS -- Offline");
                    let _ = app.emit("ipc-disconnected", ());
                    let _ = app.emit("ipc-state", "offline");
                }
                IpcEvent::State(s) => {
                    let listening = s == "listening";
                    is_listening.store(listening, Ordering::Relaxed);
                    set_state(&s);
                    let _ = app.emit("ipc-state", s);
                }
                IpcEvent::ResponseChunk { content, done } => {
                    let _ = app.emit("ipc-chunk", ChunkPayload { content, done });
                    if done {
                        set_state("idle");
                        let _ = app.emit("ipc-state", "idle");
                    }
                }
                IpcEvent::WakeWordDetected => {
                    is_listening.store(true, Ordering::Relaxed);
                    set_state("listening");
                    let _ = app.emit("ipc-wake", ());
                    let _ = app.emit("ipc-state", "listening");
                }
                IpcEvent::ConfirmationRequest { id, tool_names } => {
                    let _ = app.emit("ipc-confirm", ConfirmPayload { id, tool_names });
                }
                IpcEvent::ConfirmationList(list) => {
                    let _ = app.emit("ipc-confirmation-list", list);
                }
                IpcEvent::Ack => {}
                IpcEvent::ConfigUpdated { key, value } => {
                    if key == "WAKE_CHIME_PATH" {
                        if let Some((_, marker_path)) = wake_chime_bootstrap.take() {
                            if let Err(e) = std::fs::write(&marker_path, b"1") {
                                eprintln!("jarvis-ui: failed to write wake chime marker: {e}");
                            }
                        }
                    }
                    // Beyond the one-time bootstrap side effect above, every
                    // config_updated is also forwarded to the frontend so the
                    // settings panel (jarvisos-app#12) can reflect writes it
                    // didn't itself just make (e.g. another connected client
                    // changing CONFIRMATION_MODE).
                    let _ = app.emit("ipc-config-updated", ConfigUpdatedPayload { key, value });
                }
                IpcEvent::ConfigError { key, message } => {
                    if key == "WAKE_CHIME_PATH" && wake_chime_bootstrap.is_some() {
                        eprintln!(
                            "jarvis-ui: daemon rejected default wake chime override: {message}"
                        );
                    }
                    let _ = app.emit("ipc-config-error", ConfigErrorPayload { key, message });
                }
                IpcEvent::Error(_) => {}
                IpcEvent::SessionList(sessions) => {
                    let _ = app.emit("ipc-session-list", sessions);
                }
                IpcEvent::SessionSwitched { session, messages } => {
                    let _ = app.emit(
                        "ipc-session-switched",
                        SessionSwitchedPayload { session, messages },
                    );
                }
                IpcEvent::SessionError(message) => {
                    let _ = app.emit("ipc-session-error", message);
                }
                IpcEvent::Settings {
                    confirmation_mode,
                    wake_chime_path,
                } => {
                    let _ = app.emit(
                        "ipc-settings",
                        SettingsPayload {
                            confirmation_mode,
                            wake_chime_path,
                        },
                    );
                }
                IpcEvent::ProviderList(providers) => {
                    let _ = app.emit("ipc-provider-list", providers);
                }
                IpcEvent::ProviderError(message) => {
                    let _ = app.emit("ipc-provider-error", message);
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "jarvis-ui-test-{}-{}-{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn pending_when_marker_absent_and_resource_present() {
        let dir = temp_dir("pending");
        let marker = dir.join(WAKE_CHIME_MARKER_FILE);
        let resource = dir.join("wake_chime.wav");
        fs::write(&resource, b"fake wav bytes").unwrap();

        let result = wake_chime_bootstrap_decision(marker.clone(), resource.clone());
        assert_eq!(
            result,
            Some((resource.to_string_lossy().into_owned(), marker))
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn none_when_marker_already_written() {
        let dir = temp_dir("already-done");
        let marker = dir.join(WAKE_CHIME_MARKER_FILE);
        let resource = dir.join("wake_chime.wav");
        fs::write(&marker, b"1").unwrap();
        fs::write(&resource, b"fake wav bytes").unwrap();

        assert_eq!(wake_chime_bootstrap_decision(marker, resource), None);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn none_when_bundled_resource_is_missing() {
        let dir = temp_dir("missing-resource");
        let marker = dir.join(WAKE_CHIME_MARKER_FILE);
        let resource = dir.join("wake_chime.wav"); // never created

        assert_eq!(wake_chime_bootstrap_decision(marker, resource), None);
        fs::remove_dir_all(&dir).unwrap();
    }
}
