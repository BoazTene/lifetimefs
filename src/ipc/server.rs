use anyhow::Result;
use serde::Deserialize;

use serde_json::{Deserializer, Value};
use std::collections::HashMap;
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::{Arc, Mutex};
use std::thread;

use super::SOCKET_PATH;
use super::command::Command;

type Handler = Box<dyn Fn(Value) + Send + Sync + 'static>;
type Handlers = HashMap<String, Handler>;

pub struct IPCServer {
    handlers: Arc<Mutex<Handlers>>,
}

impl IPCServer {
    pub fn new() -> Result<IPCServer> {
        if std::fs::exists(SOCKET_PATH)? {
            std::fs::remove_file(SOCKET_PATH)?;
        }

        Ok(IPCServer {
            handlers: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub fn register<F>(&self, action: &str, callback: F)
    where
        F: Fn(serde_json::Value) + Send + Sync + 'static,
    {
        let mut handlers = self.handlers.lock().unwrap();
        handlers.insert(action.to_string(), Box::new(callback));
    }

    fn handle_stream(stream: UnixStream, handlers: Arc<Mutex<Handlers>>) {
        if let Ok(cmd) = Command::deserialize(&mut Deserializer::from_reader(stream)) {
            if let Ok(handlers) = handlers.lock() {
                if let Some(callback) = handlers.get(&cmd.action) {
                    callback(cmd.params);
                }
            }
        }
    }

    pub fn run(&self) -> Result<()> {
        match UnixListener::bind(SOCKET_PATH) {
            Err(e) => eprintln!("Could not bind listener: {}", e),
            Ok(listener) => {
                println!("Server listening on {:?}", SOCKET_PATH);

                for conn in listener.incoming() {
                    match conn {
                        Err(e) => eprintln!("Could not accept connection: {}", e),
                        Ok(stream) => {
                            // Spawn a new thread to handle each incoming connection
                            let handlers = Arc::clone(&self.handlers);

                            thread::spawn(|| IPCServer::handle_stream(stream, handlers));
                        }
                    }
                }
            }
        };

        Ok(())
    }
}
