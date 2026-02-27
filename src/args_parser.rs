use clap::{Parser, Subcommand};
use std::path::PathBuf;


#[derive(Parser)]
#[command[author="boaztene", about="A time-traveling filesystem written in Rust."]]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    Service {},

    Mount {
        mountpoint: PathBuf,
    },
}