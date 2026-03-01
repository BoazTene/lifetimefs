use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::filesystem::Lifetimefs;
use crate::ipc::IPCServer;
use crate::ipc::command::{CommandActions, MountCommand};

use anyhow::Result;
use fuser::BackgroundSession;
use serde::Deserialize;
use serde_json::Value;

pub struct Service {
    ipc: IPCServer,
    sessions: Arc<Mutex<HashMap<usize, BackgroundSession>>>,
    next_session_id: Arc<AtomicUsize>,
}

impl Service {
    pub fn new() -> Result<Service> {
        let sessions = Arc::new(Mutex::new(HashMap::new()));

        Ok(Service {
            ipc: IPCServer::new()?,
            sessions,
            next_session_id: Arc::new(AtomicUsize::new(0)),
        })
    }

    fn handle_destroy(
        sessions: &Arc<std::sync::Mutex<HashMap<usize, BackgroundSession>>>,
        id: usize,
    ) {
        if let Ok(mut sessions) = sessions.lock() {
            sessions.remove(&id);
        }

    }

    fn handle_mount(
        sessions: &Arc<std::sync::Mutex<HashMap<usize, BackgroundSession>>>,
        id: usize,
        value: Value,
    ) {
        let mount_command = match MountCommand::deserialize(value) {
            Ok(command) => command,
            Err(error) => {
                eprintln!("Invalid mount command: {error}");
                return;
            }
        };

        let mountpoint = match PathBuf::from_str(&mount_command.mountpoint) {
            Ok(path) => path,
            Err(error) => {
                eprintln!("Invalid mountpoint path '{}': {error}", mount_command.mountpoint);
                return;
            }
        };

        let sessions_clone = Arc::clone(sessions);
        let filesystem = match Lifetimefs::new(
            Box::new(move || {
                Service::handle_destroy(&sessions_clone, id);
            }),
            &mountpoint,
        ) {
            Ok(filesystem) => filesystem,
            Err(error) => {
                eprintln!("Failed to initialize filesystem for {}: {error}", mountpoint.display());
                return;
            }
        };

        let session = match filesystem.mount() {
            Ok(session) => session,
            Err(error) => {
                eprintln!("Failed to mount {}: {error}", mountpoint.display());
                return;
            }
        };

        if let Ok(mut sessions) = sessions.lock() {
            sessions.insert(id, session);
        };
    }

    fn initialize(&mut self) {
        let sessions = Arc::clone(&self.sessions);
        let next_session_id = Arc::clone(&self.next_session_id);

        self.ipc
            .register(&CommandActions::Mount.to_string(), move |v| {
                let id = next_session_id.fetch_add(1, Ordering::Relaxed);
                Service::handle_mount(&sessions, id, v);
            });
    }

    pub fn run(&mut self) -> Result<()> {
        self.initialize();

        self.ipc.run()?;

        Ok(())
    }
}
