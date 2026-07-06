//! JARVIS IPC Client
//!
//! Runs a background thread that connects to the JARVIS daemon.
//! Unix: connects via Unix domain socket.
//! Windows: reads a `.port` file and connects via TCP to 127.0.0.1.
//! Auto-reconnects every 2s on disconnect.
//!
//! IPC thread  -> Qt thread : event_rx  (mpsc Receiver<IpcEvent>)
//! Qt thread   -> IPC thread: command_tx (mpsc Sender<IpcCommand>)

use std::io::{BufRead, BufReader, Write};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::net::UnixStream;
#[cfg(unix)]
type IpcStream = UnixStream;

#[cfg(windows)]
use std::net::TcpStream;
#[cfg(windows)]
type IpcStream = TcpStream;

const RECONNECT_DELAY: Duration = Duration::from_secs(2);
const COMMAND_POLL: Duration = Duration::from_millis(50);

fn default_socket_path() -> String {
    let dir = jarvis_data_dir();
    format!("{}{}{}", dir, std::path::MAIN_SEPARATOR, "jarvis.sock")
}

/// Mirrors Project-JARVIS's per-platform data_dir() so sockets land in the
/// same directory the daemon uses.
pub fn jarvis_data_dir() -> String {
    if let Ok(d) = std::env::var("JARVIS_DATA_DIR") {
        return d;
    }
    #[cfg(target_os = "linux")]
    {
        let base = std::env::var("XDG_DATA_HOME").unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
            format!("{}/.local/share", home)
        });
        format!("{}/jarvis", base)
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        format!("{}/Library/Application Support/jarvis", home)
    }
    #[cfg(windows)]
    {
        let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| {
            let home = std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\Users\\Default".into());
            format!("{}\\AppData\\Local", home)
        });
        format!("{}\\jarvis", base)
    }
}

fn socket_path() -> String {
    std::env::var("JARVIS_SOCKET").unwrap_or_else(|_| default_socket_path())
}

#[cfg(unix)]
fn connect_to_daemon() -> std::io::Result<IpcStream> {
    IpcStream::connect(socket_path())
}

#[cfg(windows)]
fn connect_to_daemon() -> std::io::Result<IpcStream> {
    let path = socket_path();
    let port_file = format!("{}.port", path);
    let port: u16 = std::fs::read_to_string(&port_file)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::NotFound, e))?
        .trim()
        .parse()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    IpcStream::connect(("127.0.0.1", port))
}

#[derive(Debug, Clone)]
pub enum IpcEvent {
    Connected,
    Disconnected,
    State(String),
    ResponseChunk {
        content: String,
        done: bool,
    },
    WakeWordDetected,
    ConfirmationRequest {
        id: String,
        tool_names: Vec<String>,
    },
    ConfirmationList(Vec<serde_json::Value>),
    Ack,
    Error(String),
    SessionList(Vec<serde_json::Value>),
    SessionSwitched {
        session: serde_json::Value,
        messages: Vec<serde_json::Value>,
    },
    SessionError(String),
}

#[derive(Debug)]
pub enum IpcCommand {
    SendMessage(String),
    StartListening,
    StopListening,
    StopStream,
    ConfirmationResponse { id: String, approved: bool },
    ListConfirmations,
    ApproveConfirmation { id: String },
    DenyConfirmation { id: String },
    ApproveAllConfirmations,
    ListSessions,
    CreateSession { title: Option<String> },
    SwitchSession { id: String },
    RenameSession { id: String, title: String },
    DeleteSession { id: String },
    Shutdown,
}

pub struct IpcClient {
    event_rx: Receiver<IpcEvent>,
    command_tx: Sender<IpcCommand>,
}

impl IpcClient {
    pub fn new() -> Self {
        let (event_tx, event_rx) = mpsc::channel::<IpcEvent>();
        let (command_tx, command_rx) = mpsc::channel::<IpcCommand>();

        thread::Builder::new()
            .name("jarvis-ipc".into())
            .spawn(move || ipc_thread(event_tx, command_rx))
            .expect("failed to spawn IPC thread");

        Self {
            event_rx,
            command_tx,
        }
    }

    pub fn try_recv(&self) -> Option<IpcEvent> {
        match self.event_rx.try_recv() {
            Ok(event) => Some(event),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(IpcEvent::Disconnected),
        }
    }

    pub fn send_message(&self, content: String) {
        let _ = self.command_tx.send(IpcCommand::SendMessage(content));
    }

    pub fn start_listening(&self) {
        let _ = self.command_tx.send(IpcCommand::StartListening);
    }

    pub fn stop_listening(&self) {
        let _ = self.command_tx.send(IpcCommand::StopListening);
    }

    pub fn stop_stream(&self) {
        let _ = self.command_tx.send(IpcCommand::StopStream);
    }

    pub fn confirmation_response(&self, id: String, approved: bool) {
        let _ = self
            .command_tx
            .send(IpcCommand::ConfirmationResponse { id, approved });
    }

    pub fn list_confirmations(&self) {
        let _ = self.command_tx.send(IpcCommand::ListConfirmations);
    }

    pub fn approve_confirmation(&self, id: String) {
        let _ = self.command_tx.send(IpcCommand::ApproveConfirmation { id });
    }

    pub fn deny_confirmation(&self, id: String) {
        let _ = self.command_tx.send(IpcCommand::DenyConfirmation { id });
    }

    pub fn approve_all_confirmations(&self) {
        let _ = self.command_tx.send(IpcCommand::ApproveAllConfirmations);
    }

    pub fn list_sessions(&self) {
        let _ = self.command_tx.send(IpcCommand::ListSessions);
    }

    pub fn create_session(&self, title: Option<String>) {
        let _ = self.command_tx.send(IpcCommand::CreateSession { title });
    }

    pub fn switch_session(&self, id: String) {
        let _ = self.command_tx.send(IpcCommand::SwitchSession { id });
    }

    pub fn rename_session(&self, id: String, title: String) {
        let _ = self
            .command_tx
            .send(IpcCommand::RenameSession { id, title });
    }

    pub fn delete_session(&self, id: String) {
        let _ = self.command_tx.send(IpcCommand::DeleteSession { id });
    }
}

impl Default for IpcClient {
    fn default() -> Self {
        Self::new()
    }
}

fn ipc_thread(event_tx: Sender<IpcEvent>, command_rx: Receiver<IpcCommand>) {
    loop {
        match connect_to_daemon() {
            Ok(stream) => {
                let _ = event_tx.send(IpcEvent::Connected);
                run_connected(&stream, &event_tx, &command_rx);
                let _ = event_tx.send(IpcEvent::Disconnected);
            }
            Err(_) => {}
        }

        match command_rx.try_recv() {
            Ok(IpcCommand::Shutdown) | Err(TryRecvError::Disconnected) => return,
            _ => {}
        }

        thread::sleep(RECONNECT_DELAY);
    }
}

fn run_connected(
    stream: &IpcStream,
    event_tx: &Sender<IpcEvent>,
    command_rx: &Receiver<IpcCommand>,
) {
    let read_stream = match stream.try_clone() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("jarvis-ipc: clone failed: {e}");
            return;
        }
    };

    let (read_tx, read_rx) = mpsc::channel::<Option<IpcEvent>>();
    thread::Builder::new()
        .name("jarvis-ipc-reader".into())
        .spawn(move || {
            let reader = BufReader::new(read_stream);
            for line in reader.lines() {
                match line {
                    Ok(l) if !l.is_empty() => {
                        if read_tx.send(Some(parse_daemon_message(&l))).is_err() {
                            break;
                        }
                    }
                    Ok(_) => {}
                    Err(_) => {
                        let _ = read_tx.send(None);
                        break;
                    }
                }
            }
        })
        .expect("failed to spawn reader thread");

    let mut write_stream = match stream.try_clone() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("jarvis-ipc: write clone failed: {e}");
            return;
        }
    };

    loop {
        loop {
            match read_rx.try_recv() {
                Ok(Some(event)) => {
                    if event_tx.send(event).is_err() {
                        return;
                    }
                }
                Ok(None) => return,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }

        match command_rx.recv_timeout(COMMAND_POLL) {
            Ok(IpcCommand::SendMessage(content)) => {
                let msg = format!(
                    "{}\n",
                    serde_json::json!({"type":"message","content":content})
                );
                if write_stream.write_all(msg.as_bytes()).is_err() {
                    return;
                }
            }
            Ok(IpcCommand::StartListening) => {
                let msg = format!("{}\n", serde_json::json!({"type":"start_listening"}));
                if write_stream.write_all(msg.as_bytes()).is_err() {
                    return;
                }
            }
            Ok(IpcCommand::StopListening) => {
                let msg = format!("{}\n", serde_json::json!({"type":"stop_listening"}));
                let _ = write_stream.write_all(msg.as_bytes());
            }
            Ok(IpcCommand::StopStream) => {
                let msg = format!("{}\n", serde_json::json!({"type":"stop_stream"}));
                let _ = write_stream.write_all(msg.as_bytes());
            }
            Ok(IpcCommand::ConfirmationResponse { id, approved }) => {
                let msg = format!(
                    "{}\n",
                    serde_json::json!({"type":"confirmation_response","id":id,"approved":approved})
                );
                let _ = write_stream.write_all(msg.as_bytes());
            }
            Ok(IpcCommand::ListConfirmations) => {
                let msg = format!("{}\n", serde_json::json!({"type":"list_confirmations"}));
                let _ = write_stream.write_all(msg.as_bytes());
            }
            Ok(IpcCommand::ApproveConfirmation { id }) => {
                let msg = format!(
                    "{}\n",
                    serde_json::json!({"type":"approve_confirmation","id":id})
                );
                let _ = write_stream.write_all(msg.as_bytes());
            }
            Ok(IpcCommand::DenyConfirmation { id }) => {
                let msg = format!(
                    "{}\n",
                    serde_json::json!({"type":"deny_confirmation","id":id})
                );
                let _ = write_stream.write_all(msg.as_bytes());
            }
            Ok(IpcCommand::ApproveAllConfirmations) => {
                let msg = format!(
                    "{}\n",
                    serde_json::json!({"type":"approve_all_confirmations"})
                );
                let _ = write_stream.write_all(msg.as_bytes());
            }
            Ok(IpcCommand::ListSessions) => {
                let msg = format!("{}\n", serde_json::json!({"type":"list_sessions"}));
                let _ = write_stream.write_all(msg.as_bytes());
            }
            Ok(IpcCommand::CreateSession { title }) => {
                let msg = format!(
                    "{}\n",
                    serde_json::json!({"type":"create_session","title":title})
                );
                let _ = write_stream.write_all(msg.as_bytes());
            }
            Ok(IpcCommand::SwitchSession { id }) => {
                let msg = format!("{}\n", serde_json::json!({"type":"switch_session","id":id}));
                let _ = write_stream.write_all(msg.as_bytes());
            }
            Ok(IpcCommand::RenameSession { id, title }) => {
                let msg = format!(
                    "{}\n",
                    serde_json::json!({"type":"rename_session","id":id,"title":title})
                );
                let _ = write_stream.write_all(msg.as_bytes());
            }
            Ok(IpcCommand::DeleteSession { id }) => {
                let msg = format!("{}\n", serde_json::json!({"type":"delete_session","id":id}));
                let _ = write_stream.write_all(msg.as_bytes());
            }
            Ok(IpcCommand::Shutdown) => return,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
    }
}

fn parse_daemon_message(line: &str) -> IpcEvent {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
        return IpcEvent::Error(format!("invalid JSON: {line}"));
    };

    match value.get("type").and_then(|t| t.as_str()) {
        Some("state") => IpcEvent::State(
            value
                .get("state")
                .and_then(|s| s.as_str())
                .unwrap_or("idle")
                .to_string(),
        ),
        Some("response") => IpcEvent::ResponseChunk {
            content: value
                .get("content")
                .and_then(|c| c.as_str())
                .unwrap_or("")
                .to_string(),
            done: value.get("done").and_then(|d| d.as_bool()).unwrap_or(true),
        },
        Some("wake_word_detected") => IpcEvent::WakeWordDetected,
        Some("confirmation_request") => IpcEvent::ConfirmationRequest {
            id: value
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            tool_names: value
                .get("tools")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|t| t.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
        },
        Some("confirmation_list") => IpcEvent::ConfirmationList(
            value
                .get("confirmations")
                .and_then(|c| c.as_array())
                .cloned()
                .unwrap_or_default(),
        ),
        Some("ack") => IpcEvent::Ack,
        Some("error") => IpcEvent::Error(
            value
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error")
                .to_string(),
        ),
        Some("session_list") => IpcEvent::SessionList(
            value
                .get("sessions")
                .and_then(|s| s.as_array())
                .cloned()
                .unwrap_or_default(),
        ),
        Some("session_switched") => IpcEvent::SessionSwitched {
            session: value
                .get("session")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
            messages: value
                .get("messages")
                .and_then(|m| m.as_array())
                .cloned()
                .unwrap_or_default(),
        },
        Some("session_error") => IpcEvent::SessionError(
            value
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown session error")
                .to_string(),
        ),
        Some("ping") | Some("pong") => IpcEvent::State("idle".into()),
        other => IpcEvent::Error(format!("unknown type: {other:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;
    use std::os::unix::net::UnixStream as TestUnixStream;
    use std::time::{Duration, Instant};

    fn unique_sock_path(name: &str) -> String {
        format!(
            "/tmp/jarvis-ipc-test-{}-{}-{}.sock",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )
    }

    fn read_line(reader: &mut BufReader<TestUnixStream>) -> serde_json::Value {
        let mut buf = String::new();
        reader.read_line(&mut buf).unwrap();
        serde_json::from_str(buf.trim_end()).unwrap()
    }

    fn wait_for_event(client: &IpcClient, pred: impl Fn(&IpcEvent) -> bool) -> IpcEvent {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(ev) = client.try_recv() {
                if pred(&ev) {
                    return ev;
                }
            }
            if Instant::now() > deadline {
                panic!("timed out waiting for expected IpcEvent");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    // Exercises the real IpcClient background threads end-to-end over an
    // actual Unix socket -- not a mock -- so it catches wire-format drift
    // between this client and jarvis's confirmation-query protocol.
    #[test]
    fn confirmation_commands_round_trip_over_real_socket() {
        let path = unique_sock_path("confirmations");
        let _ = std::fs::remove_file(&path);
        std::env::set_var("JARVIS_SOCKET", &path);

        let listener = UnixListener::bind(&path).unwrap();
        let client = IpcClient::new();

        let (stream, _) = listener.accept().unwrap();
        let mut write_half = stream.try_clone().unwrap();
        let mut reader = BufReader::new(stream);

        wait_for_event(&client, |e| matches!(e, IpcEvent::Connected));

        client.list_confirmations();
        assert_eq!(
            read_line(&mut reader),
            serde_json::json!({"type": "list_confirmations"})
        );

        client.approve_confirmation("req1".into());
        assert_eq!(
            read_line(&mut reader),
            serde_json::json!({"type": "approve_confirmation", "id": "req1"})
        );

        client.deny_confirmation("req2".into());
        assert_eq!(
            read_line(&mut reader),
            serde_json::json!({"type": "deny_confirmation", "id": "req2"})
        );

        client.approve_all_confirmations();
        assert_eq!(
            read_line(&mut reader),
            serde_json::json!({"type": "approve_all_confirmations"})
        );

        write_half
            .write_all(
                b"{\"type\":\"confirmation_list\",\"confirmations\":\
                   [{\"id\":\"req1\",\"tool_names\":[\"fs.delete\"],\"created_at\":1.0}]}\n",
            )
            .unwrap();
        match wait_for_event(&client, |e| matches!(e, IpcEvent::ConfirmationList(_))) {
            IpcEvent::ConfirmationList(list) => {
                assert_eq!(list.len(), 1);
                assert_eq!(list[0]["id"], "req1");
            }
            _ => unreachable!(),
        }

        write_half
            .write_all(b"{\"type\":\"ack\",\"message\":\"Approved req1\"}\n")
            .unwrap();
        assert!(matches!(
            wait_for_event(&client, |e| matches!(e, IpcEvent::Ack)),
            IpcEvent::Ack
        ));

        write_half
            .write_all(b"{\"type\":\"confirmation_request\",\"id\":\"req3\",\"tools\":[\"net.request\"]}\n")
            .unwrap();
        match wait_for_event(&client, |e| {
            matches!(e, IpcEvent::ConfirmationRequest { .. })
        }) {
            IpcEvent::ConfirmationRequest { id, tool_names } => {
                assert_eq!(id, "req3");
                assert_eq!(tool_names, vec!["net.request".to_string()]);
            }
            _ => unreachable!(),
        }

        std::env::remove_var("JARVIS_SOCKET");
        let _ = std::fs::remove_file(&path);
    }
}
