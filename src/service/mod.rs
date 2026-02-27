use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;
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
    next_session_id: usize,
}

impl Service {
    pub fn new() -> Result<Service> {
        let sessions = Arc::new(Mutex::new(HashMap::new()));

        Ok(Service {
            ipc: IPCServer::new()?,
            sessions,
            next_session_id: 0,
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
        let sessions_clone = Arc::clone(sessions);

        if let Ok(mut sessions) = sessions.lock() {
            if let Ok(mount_command) = MountCommand::deserialize(value) {
                if let Ok(mountpoint) = &PathBuf::from_str(&mount_command.mountpoint) {
                    if let Ok(filesystem) = Lifetimefs::new(
                        Box::new(move || {
                            Service::handle_destroy(&sessions_clone, id);
                        }),
                        mountpoint,
                    ) {
                        if let Ok(session) = filesystem.mount() {
                            sessions.insert(id, session);
                        }
                    }
                }
            }
        };
    }

    fn initialize(&mut self) {
        let sessions = Arc::clone(&self.sessions);
        let id = self.next_session_id;
        self.next_session_id += 1;

        self.ipc
            .register(&CommandActions::Mount.to_string(), move |v| {
                Service::handle_mount(&sessions, id, v);
            });
    }

    pub fn run(&mut self) -> Result<()> {
        self.initialize();

        self.ipc.run()?;

        Ok(())
    }
}
