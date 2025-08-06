use anyhow::Result;
use clap::{Parser, Subcommand};

mod run;
mod trigger;

#[derive(Subcommand, Debug)]
pub enum Command {
    Trigger {
        name: String,
    },
    Run {
        path: String
    },
}

#[derive(Parser, Debug)]
struct Commands {
    #[command(subcommand)]
    command: Command,
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

pub async fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Trigger { name } => {
            trigger::run(name).await?
        }
        Command::Run { path } => {
            run::run(path).await?;
        }
    }

    Ok(())
}
