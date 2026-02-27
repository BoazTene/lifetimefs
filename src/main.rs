use anyhow::Result;
use clap::{CommandFactory, Parser};
use clap_help::Printer;

mod service;
mod args_parser;
mod filesystem;
mod filesystem_client;
mod ipc;

use args_parser::{Cli, Commands};
use service::Service;
use filesystem_client::FilesystemClient;
use ipc::IPCClient;


fn main() -> Result<()> {
    let args = Cli::parse();

    match &args.command {
        Some(Commands::Service {  }) => {
            let mut service = Service::new()?;
            service.run()?;
        }

        Some(Commands::Mount { mountpoint }) => {
            let mut client = FilesystemClient::new()?;

            if let Some(mountpoint) = mountpoint.to_str() {
                client.mount(mountpoint)?;
            } else {
                Printer::new(Cli::command()).print_help();
            }
        }

        None => {
            Printer::new(Cli::command()).print_help();
        }
    }

    Ok(())
}
