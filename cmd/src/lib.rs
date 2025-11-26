use std::fmt::Display;
use std::path::Path;
use std::process::Stdio;
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
    stdout: bool,
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
            stdout: false,
        }
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

    pub fn env<K, V>(&mut self, key: K, value: V) -> &mut Self
    where
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.cmd.env(key, value);
        self
    }

    pub fn envs<I, K, V>(&mut self, vars: I) -> &mut Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.cmd.envs(vars);
        self
    }

    pub fn current_dir<P: AsRef<Path>>(&mut self, dir: P) -> &mut Command {
        self.cmd.current_dir(dir);
        self
    }

    pub fn stdout(&mut self, stdout: bool) -> &mut Command {
        self.stdout = stdout;
        self
    }

    pub fn get_stdout(&self) -> bool {
        self.stdout
    }

    pub fn sudo(self) -> Self {
        let mut privileged_cmd = Command::new("sudo");

        let cmd = self.cmd.as_std();

        privileged_cmd
            .arg("-n") // non-interactive
            .arg(cmd.get_program())
            .args(cmd.get_args())
            .stdout(self.get_stdout());

        for env in cmd.get_envs() {
            if let (key, Some(value)) = env {
                privileged_cmd.env(key, value);
            }
        }

        if let Some(dir) = cmd.get_current_dir() {
            privileged_cmd.current_dir(dir);
        }

        privileged_cmd
    }

    pub fn spawn(&mut self) -> Result<Child, CommandError> {
        self.cmd
            .stdin(Stdio::piped())
            .stdout(if self.stdout {
                Stdio::inherit()
            } else {
                Stdio::piped()
            })
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| CommandError::Spawn {
                command: self.to_string(),
                error,
            })
    }

    pub async fn output(&mut self) -> Result<Output, CommandError> {
        self.cmd
            .stdin(Stdio::piped())
            .stdout(if self.stdout {
                Stdio::inherit()
            } else {
                Stdio::piped()
            })
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|error| CommandError::Spawn {
                command: self.to_string(),
                error,
            })
    }

    pub async fn run(&mut self) -> Result<Output, CommandError> {
        self.output().await.and_then(|out| {
            if out.status.success() {
                Ok(out)
            } else {
                Err(CommandError::Failure {
                    command: self.to_string(),
                    stderr: String::from_utf8_lossy(&out.stderr).to_string(),
                })
            }
        })
    }

    pub async fn handle<OutHandler, ErrHandler, HandlerValue, HandlerError>(
        &mut self,
        stdout_handler: OutHandler,
        stderr_handler: ErrHandler,
    ) -> Result<Result<HandlerValue, HandlerError>, CommandError>
    where
        ErrHandler: Fn(&Vec<u8>) -> Result<Option<HandlerValue>, HandlerError>,
        OutHandler: Fn(&Vec<u8>) -> Result<HandlerValue, HandlerError>,
    {
        self.output().await.and_then(|output| {
            if output.status.success() {
                return Ok(stdout_handler(&output.stdout));
            }

            match stderr_handler(&output.stderr) {
                Err(error) => Ok(Err(error)),
                Ok(Some(value)) => Ok(Ok(value)),
                Ok(None) => Err(CommandError::Failure {
                    command: self.to_string(),
                    stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                }),
            }
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
