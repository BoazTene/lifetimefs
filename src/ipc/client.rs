use anyhow::{ Result };
use std::io::Write;
use std::os::unix::net::UnixStream;

use super::command::Command;
use super::SOCKET_PATH;

pub struct IPCClient {
    stream: UnixStream,
}

impl IPCClient {
    pub fn new() -> Result<IPCClient> {
        let stream = UnixStream::connect(SOCKET_PATH)?;

        Ok(IPCClient { stream: stream })
    }

    pub fn send_command(&mut self, command: &Command) -> Result<()> {
        self.stream.write_all(serde_json::to_string(&command)?.as_bytes())?;

        Ok(())
    }
}

