use clap::{CommandFactory, Parser, Subcommand};
use clap_help::Printer;
use std::path::PathBuf;


#[derive(Parser)]
#[command[author="boaztene", about="A time-traveling filesystem written in Rust."]]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Mount {
        mount_point: PathBuf,
    },
}

fn main() {
    let args = Cli::parse();

    match &args.command {
        Some(Commands::Mount { mount_point }) => {
            println!("mounting at: {:?}", mount_point);
        }

        None => {
            Printer::new(Cli::command()).print_help();
        }
    }
}
