use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

use crate::ipc::protocol::IpcMessage;

pub struct IpcServer {
    clients: Arc<Mutex<Vec<TcpStream>>>,
}

impl IpcServer {
    pub fn start(port: u16) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", port)).expect("Failed to bind IPC port");

        let clients = Arc::new(Mutex::new(Vec::new()));
        let clients_clone = clients.clone();

        std::thread::spawn(move || {
            for stream in listener.incoming() {
                if let Ok(stream) = stream {
                    clients_clone.lock().unwrap().push(stream);
                }
            }
        });

        Self { clients }
    }

    pub fn broadcast(&self, msg: &IpcMessage) {
        let json = serde_json::to_string(msg).unwrap();
        let mut clients = self.clients.lock().unwrap();

        clients.retain_mut(|stream| {
            stream.write_all(json.as_bytes()).is_ok() && stream.write_all(b"\n").is_ok()
        });
    }
}
