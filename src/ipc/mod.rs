pub(crate) mod command;

pub mod server;
pub mod client;

pub use client::IPCClient;
pub use server::IPCServer;

const SOCKET_PATH: &str = "/tmp/lifetimefs.sock";