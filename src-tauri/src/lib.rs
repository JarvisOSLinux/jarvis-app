mod ipc;

use ipc::{IpcClient, IpcEvent};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

struct AppState {
    ipc: Arc<Mutex<IpcClient>>,
    is_listening: Arc<AtomicBool>,
}

#[derive(Serialize, Clone)]
struct ChunkPayload {
    content: String,
    done: bool,
}

#[derive(Serialize, Clone)]
struct ConfirmPayload {
    id: String,
    description: String,
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let ipc = Arc::new(Mutex::new(IpcClient::new()));
    let ipc_poll = ipc.clone();
    let is_listening = Arc::new(AtomicBool::new(false));
    let is_listening_poll = is_listening.clone();

    tauri::Builder::default()
        .manage(AppState { ipc, is_listening })
        .setup(|app| {
            let handle = app.handle().clone();
            std::thread::Builder::new()
                .name("jarvis-ipc-poll".into())
                .spawn(move || poll_loop(ipc_poll, is_listening_poll, handle))
                .expect("failed to spawn IPC poll thread");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            send_message,
            toggle_listening,
            stop_stream,
            send_confirmation_response,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Tauri application");
}

fn poll_loop(ipc: Arc<Mutex<IpcClient>>, is_listening: Arc<AtomicBool>, app: AppHandle) {
    loop {
        loop {
            let event = ipc.lock().unwrap().try_recv();
            let Some(ev) = event else { break };
            match ev {
                IpcEvent::Connected => {
                    let _ = app.emit("ipc-connected", ());
                    let _ = app.emit("ipc-state", "idle");
                }
                IpcEvent::Disconnected => {
                    is_listening.store(false, Ordering::Relaxed);
                    let _ = app.emit("ipc-disconnected", ());
                    let _ = app.emit("ipc-state", "offline");
                }
                IpcEvent::State(s) => {
                    let listening = s == "listening";
                    is_listening.store(listening, Ordering::Relaxed);
                    let _ = app.emit("ipc-state", s);
                }
                IpcEvent::ResponseChunk { content, done } => {
                    let _ = app.emit("ipc-chunk", ChunkPayload { content, done });
                    if done {
                        let _ = app.emit("ipc-state", "idle");
                    }
                }
                IpcEvent::WakeWordDetected => {
                    is_listening.store(true, Ordering::Relaxed);
                    let _ = app.emit("ipc-wake", ());
                    let _ = app.emit("ipc-state", "listening");
                }
                IpcEvent::ConfirmationRequest { id, description } => {
                    let _ = app.emit("ipc-confirm", ConfirmPayload { id, description });
                }
                IpcEvent::Error(_) => {}
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}
