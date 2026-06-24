mod bridge;
mod ipc;

use std::io::{BufRead, BufReader, Write};
use std::sync::mpsc;
use std::thread;

#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};

#[cfg(windows)]
use std::net::{TcpListener, TcpStream};

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl, QString};

#[cfg(unix)]
const CONTROL_SOCKET: &str = "/tmp/jarvis-widget.sock";

#[cfg(unix)]
const CONTROL_CMD_FILE: &str = "/tmp/jarvis-widget-cmd";

#[cfg(windows)]
fn control_socket_path() -> String {
    let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| {
        let home =
            std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\Users\\Default".into());
        format!("{}\\AppData\\Local", home)
    });
    format!("{}\\jarvis\\jarvis-widget.sock", base)
}

#[cfg(windows)]
fn control_cmd_path() -> String {
    let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| {
        let home =
            std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\Users\\Default".into());
        format!("{}\\AppData\\Local", home)
    });
    format!("{}\\jarvis\\jarvis-widget-cmd", base)
}

#[derive(Debug, PartialEq)]
enum ControlCommand {
    Toggle,
    Stop,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let command = if args.iter().any(|a| a == "--toggle") {
        Some(ControlCommand::Toggle)
    } else if args.iter().any(|a| a == "--stop") {
        Some(ControlCommand::Stop)
    } else {
        None
    };

    if let Some(cmd) = &command {
        if send_to_existing(cmd) {
            return;
        }
    }

    if command == Some(ControlCommand::Stop) {
        return;
    }

    cleanup_control_socket();

    let (ctrl_tx, ctrl_rx) = mpsc::channel::<ControlCommand>();
    start_control_listener(ctrl_tx);

    let mut app = QGuiApplication::new();
    app.set_application_name(QString::from("JARVIS Widget"));
    app.set_organization_name(QString::from("JarvisOS"));
    app.set_application_version(QString::from(env!("CARGO_PKG_VERSION")));

    let mut engine = QQmlApplicationEngine::default();

    engine.load(QUrl::from(QString::from(
        "qrc:/qt/qml/JarvisWidget/qml/Widget.qml",
    )));

    if engine.root_objects().is_empty() {
        eprintln!("jarvis-widget: failed to load Widget.qml");
        cleanup_control_socket();
        std::process::exit(1);
    }

    let cmd_file = cmd_file_path();
    thread::Builder::new()
        .name("jarvis-widget-ctrl".into())
        .spawn(move || {
            for cmd in ctrl_rx {
                let cmd_str = match cmd {
                    ControlCommand::Toggle => "toggle",
                    ControlCommand::Stop => "stop",
                };
                let _ = std::fs::write(&cmd_file, cmd_str);
            }
        })
        .ok();

    app.exec();

    cleanup_control_socket();
    let _ = std::fs::remove_file(cmd_file_path());
}

fn cmd_file_path() -> String {
    #[cfg(unix)]
    {
        CONTROL_CMD_FILE.to_string()
    }
    #[cfg(windows)]
    {
        control_cmd_path()
    }
}

fn cleanup_control_socket() {
    #[cfg(unix)]
    {
        let _ = std::fs::remove_file(CONTROL_SOCKET);
    }
    #[cfg(windows)]
    {
        let port_file = format!("{}.port", control_socket_path());
        let _ = std::fs::remove_file(&port_file);
    }
}

#[cfg(unix)]
fn send_to_existing(cmd: &ControlCommand) -> bool {
    if let Ok(mut stream) = UnixStream::connect(CONTROL_SOCKET) {
        let msg = match cmd {
            ControlCommand::Toggle => "toggle\n",
            ControlCommand::Stop => "stop\n",
        };
        let _ = stream.write_all(msg.as_bytes());
        true
    } else {
        false
    }
}

#[cfg(windows)]
fn send_to_existing(cmd: &ControlCommand) -> bool {
    let port_file = format!("{}.port", control_socket_path());
    let port: u16 = match std::fs::read_to_string(&port_file) {
        Ok(s) => match s.trim().parse() {
            Ok(p) => p,
            Err(_) => return false,
        },
        Err(_) => return false,
    };
    if let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) {
        let msg = match cmd {
            ControlCommand::Toggle => "toggle\n",
            ControlCommand::Stop => "stop\n",
        };
        let _ = stream.write_all(msg.as_bytes());
        true
    } else {
        false
    }
}

#[cfg(unix)]
fn start_control_listener(ctrl_tx: mpsc::Sender<ControlCommand>) {
    let listener = UnixListener::bind(CONTROL_SOCKET)
        .expect("failed to bind control socket");

    let _ = std::fs::set_permissions(
        CONTROL_SOCKET,
        std::os::unix::fs::PermissionsExt::from_mode(0o666),
    );

    thread::Builder::new()
        .name("jarvis-widget-listener".into())
        .spawn(move || {
            for conn in listener.incoming() {
                if let Ok(stream) = conn {
                    let reader = BufReader::new(stream);
                    for line in reader.lines().take(1) {
                        if let Ok(line) = line {
                            let cmd = match line.trim() {
                                "toggle" => Some(ControlCommand::Toggle),
                                "stop" => Some(ControlCommand::Stop),
                                _ => None,
                            };
                            if let Some(c) = cmd {
                                if ctrl_tx.send(c).is_err() { return; }
                            }
                        }
                    }
                }
            }
        })
        .expect("failed to spawn control listener");
}

#[cfg(windows)]
fn start_control_listener(ctrl_tx: mpsc::Sender<ControlCommand>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .expect("failed to bind control listener");

    let port = listener.local_addr().unwrap().port();
    let port_file = format!("{}.port", control_socket_path());
    if let Some(parent) = std::path::Path::new(&port_file).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&port_file, port.to_string());

    thread::Builder::new()
        .name("jarvis-widget-listener".into())
        .spawn(move || {
            for conn in listener.incoming() {
                if let Ok(stream) = conn {
                    let reader = BufReader::new(stream);
                    for line in reader.lines().take(1) {
                        if let Ok(line) = line {
                            let cmd = match line.trim() {
                                "toggle" => Some(ControlCommand::Toggle),
                                "stop" => Some(ControlCommand::Stop),
                                _ => None,
                            };
                            if let Some(c) = cmd {
                                if ctrl_tx.send(c).is_err() { return; }
                            }
                        }
                    }
                }
            }
        })
        .expect("failed to spawn control listener");
}
