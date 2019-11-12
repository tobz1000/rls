use failure::Fail;
use std::fmt;
use std::process::{Command, Output};
use std::time::Duration;
use std::thread::sleep;
use log::trace;

#[derive(Debug, Fail)]
pub struct CommandFail {
    command: String,
    cause: FailCause,
}

#[derive(Debug)]
enum FailCause {
    RunFail {
        err_code: Option<i32>,
        err_out: String,
    },
    ProcFail {
        err: std::io::Error,
    },
}

impl fmt::Display for CommandFail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let CommandFail { command, cause } = self;

        write!(f, "Command failed: {}", command)?;

        match cause {
            FailCause::RunFail {
                err_code: _,
                err_out,
            } => {
                if !err_out.is_empty() {
                    write!(f, "\n\tOutput: {}", err_out)?;
                }
            }
            FailCause::ProcFail { err } => {
                write!(f, "\n\tProcess failure: {}", err)?;
            }
        }

        Ok(())
    }
}

pub fn run(mut command: Command) -> Result<String, CommandFail> {
    trace!("run {:?}", &command);

    let output = map_io_err(command.output(), &command)?;

    map_output(output, &command)
}

/// Returns `Ok(Some(_))` if the command finishes before the timeout; `Ok(None)` if the command is
/// running up to the timeout and killed; `Err(_)` otherwise.
pub fn run_with_timeout(
    mut command: Command,
    timeout: Duration
) -> Result<Option<String>, CommandFail> {
    trace!("run_with_timeout={}ms {:?}", timeout.as_millis(), &command);

    let mut child = map_io_err(command.spawn(), &command)?;

    sleep(timeout);

    let proc_has_finished = map_io_err(child.try_wait(), &command)?.is_some();

    if proc_has_finished {
        let output = map_io_err(child.wait_with_output(), &command)?;

        let out_str = map_output(output, &command)?;

        Ok(Some(out_str))
    } else {
        map_io_err(child.kill(), &command)?;

        Ok(None)
    }
}

fn map_io_err<T>(
    result: Result<T, std::io::Error>,
    command: &Command
) -> Result<T, CommandFail> {
    match result {
        Ok(t) => Ok(t),
        Err(err) => Err(CommandFail {
            command: format!("{:?}", command),
            cause: FailCause::ProcFail { err }
        })
    }
}

fn map_output(
    Output { status, stdout, stderr }: Output,
    command: &Command
) -> Result<String, CommandFail> {
    if !status.success() {
        let err_out = String::from_utf8_lossy(&stderr).into_owned();

        Err(CommandFail {
            command: format!("{:?}", command),
            cause: FailCause::RunFail {
                err_code: status.code(),
                err_out,
            },
        })
    } else {
        let stdout_string = String::from_utf8_lossy(&stdout).into_owned();

        Ok(stdout_string)
    }
}