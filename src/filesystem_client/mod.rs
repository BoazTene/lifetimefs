use anyhow::Result;

use crate::IPCClient;
use crate::ipc::command::{Command, CommandActions, MountCommand};

pub struct FilesystemClient {
    ipc: IPCClient
}

impl FilesystemClient {
    pub fn new() -> Result<FilesystemClient> {
        Ok(
            FilesystemClient { 
                ipc: IPCClient::new()? 
            }
        )
    }

    pub fn mount(&mut self, mountpoint: &str) -> Result<()> {
        let command = Command { 
            action: CommandActions::Mount.to_string(), 
            params: serde_json::to_value(MountCommand {
                mountpoint: mountpoint.to_string()
            })?
        };
        
        self.ipc.send_command(&command)?;
        Ok(())
    }
}