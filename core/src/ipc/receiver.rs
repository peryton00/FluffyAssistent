use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::net::TcpListener;
// use std::process::Command;
use std::sync::Mutex;

use once_cell::sync::Lazy;
use uuid::Uuid;

use crate::ipc::command::Command as IpcCommand;
use crate::permissions::decision::PermissionDecision;
use crate::permissions::policy::evaluate;

static PENDING: Lazy<Mutex<HashMap<String, IpcCommand>>> = Lazy::new(|| Mutex::new(HashMap::new()));

pub fn start_command_server(port: u16) {
    let listener = TcpListener::bind(("127.0.0.1", port)).expect("Failed to bind command port");

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            if let Ok(stream) = stream {
                let reader = BufReader::new(stream);
                for line in reader.lines().flatten() {
                    if let Ok(cmd) = serde_json::from_str::<IpcCommand>(&line) {
                        handle_command(cmd);
                    }
                }
            }
        }
    });
}

fn handle_command(cmd: IpcCommand) {
    match cmd {
        IpcCommand::Confirm { command_id } => {
            if let Some(original) = PENDING.lock().unwrap().remove(&command_id) {
                execute(original);
            }
        }

        IpcCommand::Cancel { command_id } => {
            PENDING.lock().unwrap().remove(&command_id);
        }

        other => match evaluate(&other) {
            PermissionDecision::Allow => execute(other),

            PermissionDecision::RequireConfirmation { reason } => {
                let id = Uuid::new_v4().to_string();
                PENDING.lock().unwrap().insert(id.clone(), other.clone());

                // Broadcast confirmation request as JSON over IPC
                let payload = serde_json::json!({
                    "type": "confirm_required",
                    "command_id": id,
                    "command_name": format!("{:?}", other),
                    "details": reason
                });

                crate::ipc::server::IpcServer::broadcast_global(&crate::ipc::protocol::IpcMessage {
                    schema_version: "1.0".to_string(),
                    payload,
                });
            }

            PermissionDecision::Deny { reason } => {
                println!("[DENIED] {}", reason);
            }
        },
    }
}

fn execute(cmd: IpcCommand) {
    match cmd {
        IpcCommand::KillProcess { pid } => {
            #[cfg(target_os = "windows")]
            {
                let result = std::process::Command::new("taskkill")
                    .args(["/PID", &pid.to_string(), "/T", "/F"])
                    .output();

                let status = match result {
                    Ok(out) if out.status.success() => "success",
                    Ok(out) => {
                        eprintln!("{:?}", out.stderr);
                        "failed"
                    }
                    Err(_) => "failed",
                };

                crate::ipc::server::IpcServer::broadcast_global(&crate::ipc::protocol::IpcMessage {
                    schema_version: "1.0".to_string(),
                    payload: serde_json::json!({
                        "type": "execution_result",
                        "command": "KillProcess",
                        "pid": pid,
                        "status": status
                    }),
                });
            }
        }
        _ => {}
    }
}

