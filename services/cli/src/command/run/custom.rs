use anyhow::Result;
use serde::Deserialize;
use tokio::{io::{AsyncBufReadExt, BufReader}, process::Command};
use std::process::Stdio;

#[derive(Debug, Deserialize)]
pub struct CustomRun {
    command: String,
    output: String,
}

pub async fn process(cfg: CustomRun, working_dir: &str) -> Result<()> {
    run_command(&cfg.command, working_dir, "command").await?;
    run_command(&cfg.output, working_dir, "output").await?;
    Ok(())
}

async fn run_command(command_line: &str, working_dir: &str, label: &str) -> Result<()> {
    println!("Running {}: \"{}\", on path: \"{}\"", label, command_line, working_dir);

    let mut args = command_line.split_whitespace();
    let command = args.next().expect("there must be a command");

    let mut cmd = Command::new(command);
    cmd.args(args)
        .current_dir(working_dir)
        .stdout(Stdio::piped());

    let mut child = cmd.spawn()
        .expect("failed to spawn command");

    let stdout = child.stdout
        .take()
        .expect("child did not have a handle to stdout");

    let mut reader = BufReader::new(stdout).lines();
    while let Some(line) = reader.next_line().await? {
        println!("{}", line);
    }

    let status = child.wait().await
        .expect("child process encountered an error");

    println!("{} status was: {}", label, status);

    Ok(())
}

