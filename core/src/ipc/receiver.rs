use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::process::Command;
use std::sync::Mutex;

use once_cell::sync::Lazy;
use uuid::Uuid;

use crate::ipc::command::Command as IpcCommand;
use crate::permissions::policy::evaluate;
use crate::permissions::decision::PermissionDecision;

static PENDING: Lazy<Mutex<HashMap<String, IpcCommand>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub fn start_command_server(port: u16) {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .expect("Failed to bind command port");

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

            PermissionDecision::RequireConfirmation { .. } => {
                let id = Uuid::new_v4().to_string();
                PENDING.lock().unwrap().insert(id.clone(), other);
                println!("[CONFIRM REQUIRED] id={}", id);
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
            println!("[EXECUTE] Kill tree for PID {}", pid);

            #[cfg(target_os = "windows")]
            {
                let _ = Command::new("taskkill")
                    .args(["/PID", &pid.to_string(), "/T", "/F"])
                    .output();
            }
        }

        _ => {}
    }
}
