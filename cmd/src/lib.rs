use std::fmt::Display;
use std::{ffi::OsStr, process::Output};
use tokio::process::{Child, Command as BaseCommand};

use thiserror::Error;

#[derive(Error, Debug)]
pub enum CommandError {
    #[error("failed to spawn command: {command}")]
    Spawn {
        command: String,
        #[source]
        error: tokio::io::Error,
    },

    #[error("command failed: {command}\n{stderr}")]
    Failure { command: String, stderr: String },
}

#[derive(Debug)]
pub struct Command {
    cmd: BaseCommand,
}

impl Display for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let cmd = self.cmd.as_std();
        let program = cmd.get_program().to_str().unwrap();
        let args = cmd
            .get_args()
            .map(|a| a.to_str().unwrap())
            .collect::<Vec<_>>()
            .join(" ");
        if args.is_empty() {
            write!(f, "{program}",)
        } else {
            write!(f, "{program} {args}",)
        }
    }
}

impl Command {
    pub fn new<S: AsRef<OsStr>>(program: S) -> Self {
        Self {
            cmd: BaseCommand::new(program),
        }
    }

    #[allow(dead_code)]
    pub fn env(&mut self, key: &str, value: &str) -> &mut Self {
        self.cmd.env(key, value);
        self
    }

    pub fn arg<S: AsRef<OsStr>>(&mut self, arg: S) -> &mut Self {
        self.cmd.arg(arg);
        self
    }

    pub fn args<I, S>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.cmd.args(args);
        self
    }

    pub async fn run(&mut self) -> Result<(), CommandError> {
        self.output().await.and_then(|out| {
            if out.status.success() {
                Ok(())
            } else {
                Err(CommandError::Failure {
                    command: self.to_string(),
                    stderr: String::from_utf8_lossy(&out.stderr).to_string(),
                })
            }
        })
    }

    #[allow(dead_code)]
    pub fn spawn(&mut self) -> Result<Child, CommandError> {
        self.cmd.spawn().map_err(|error| CommandError::Spawn {
            command: self.to_string(),
            error,
        })
    }

    pub async fn output(&mut self) -> Result<Output, CommandError> {
        self.cmd
            .output()
            .await
            .map_err(|error| CommandError::Spawn {
                command: self.to_string(),
                error,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_command() {
        assert_eq!(Command::new("lusid").to_string(), "lusid")
    }

    #[test]
    fn test_get_command_with_one_arg() {
        assert_eq!(Command::new("lusid").arg("-a").to_string(), "lusid -a")
    }

    #[test]
    fn test_get_command_with_two_args() {
        assert_eq!(
            Command::new("lusid").arg("-a").arg("-b").to_string(),
            "lusid -a -b"
        )
    }
}
