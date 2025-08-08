use anyhow::Result;
use clap::{Parser, Subcommand};

mod trigger;
mod push;

#[derive(Subcommand, Debug)]
pub enum Command {
    Trigger {
        name: String,

        #[arg(short, long)]
        body: String,
    },
    Push {
        path: String,
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
        Command::Trigger { name, body } => {
            trigger::run(name, body).await?
        }
        Command::Push { path } => {
            push::run(&path).await?;
        }
    }

    Ok(())
}
