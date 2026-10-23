mod script;

use std::convert::Infallible;
use std::env::{self, VarError};
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, Write};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{self, Command};

use script::Startup;

const USAGE: &str = "usage: startish [--history CMD]... -- COMMAND...";

#[derive(Debug, PartialEq)]
struct Args {
    history: Vec<String>,
    command: Vec<String>,
}

#[derive(Debug, PartialEq)]
enum Parsed {
    Run(Args),
    Help,
    Version,
}

fn parse_args(mut args: impl Iterator<Item = String>) -> Result<Parsed, String> {
    let mut history = Vec::new();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Parsed::Help),
            "-V" | "--version" => return Ok(Parsed::Version),
            "--history" => history.push(args.next().ok_or("--history needs a value")?),
            "--" => break,
            _ => {
                if let Some(value) = arg.strip_prefix("--history=") {
                    history.push(value.to_owned());
                } else if arg.starts_with('-') {
                    return Err(format!("unknown option {arg}"));
                } else {
                    let command = std::iter::once(arg).chain(args).collect();
                    return Ok(Parsed::Run(Args { history, command }));
                }
            }
        }
    }
    let command: Vec<String> = args.collect();
    if command.is_empty() {
        return Err("missing COMMAND".to_owned());
    }
    Ok(Parsed::Run(Args { history, command }))
}

fn main() {
    let args = env::args_os()
        .skip(1)
        .map(OsString::into_string)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "arguments must be valid UTF-8".to_owned())
        .and_then(|args| parse_args(args.into_iter()));
    let args = match args {
        Ok(Parsed::Run(args)) => args,
        Ok(Parsed::Help) => {
            println!("{USAGE}");
            return;
        }
        Ok(Parsed::Version) => {
            println!("startish {}", env!("CARGO_PKG_VERSION"));
            return;
        }
        Err(msg) => {
            eprintln!("startish: {msg}\n{USAGE}");
            process::exit(2);
        }
    };
    let err = run(&args);
    eprintln!("startish: {err}");
    process::exit(1);
}

fn run(args: &Args) -> io::Error {
    let shell = env::var_os("SHELL").filter(|s| !s.is_empty());
    let name = shell
        .as_deref()
        .and_then(|s| Path::new(s).file_name())
        .and_then(|s| s.to_str());
    let command = script::command_line(&args.command);
    let startup = Startup {
        history: &args.history,
        command: &command,
    };
    let result = match (shell.as_deref(), name) {
        (Some(shell), Some("bash")) => exec_bash(shell, &startup),
        (Some(shell), Some("zsh")) => exec_zsh(shell, &startup),
        _ => fallback(shell.as_deref(), &args.command),
    };
    let Err(err) = result;
    err
}

fn utf8(path: &Path) -> io::Result<&str> {
    path.to_str()
        .ok_or_else(|| io::Error::other(format!("{} is not valid UTF-8", path.display())))
}

fn exec_bash(shell: &OsStr, startup: &Startup) -> io::Result<Infallible> {
    let (mut file, path) = tempfile::Builder::new()
        .prefix("startish-")
        .suffix(".bash")
        .tempfile()?
        .keep()?;
    let written = utf8(&path).and_then(|p| file.write_all(startup.bash_rcfile(p).as_bytes()));
    drop(file);
    let err = match written {
        Ok(()) => Command::new(shell)
            .arg("--rcfile")
            .arg(&path)
            .arg("-i")
            .exec(),
        Err(err) => err,
    };
    let _ = fs::remove_file(&path);
    Err(err)
}

fn exec_zsh(shell: &OsStr, startup: &Startup) -> io::Result<Infallible> {
    let zdotdir = match env::var("ZDOTDIR") {
        Ok(dir) => Some(dir),
        Err(VarError::NotPresent) => None,
        Err(VarError::NotUnicode(_)) => {
            return Err(io::Error::other("ZDOTDIR is not valid UTF-8"));
        }
    };
    let dir: PathBuf = tempfile::Builder::new()
        .prefix("startish-")
        .tempdir()?
        .keep();
    let written = utf8(&dir).and_then(|d| {
        fs::write(dir.join(".zshenv"), script::zshenv(d, zdotdir.as_deref()))?;
        fs::write(dir.join(".zshrc"), startup.zshrc(d))
    });
    let err = match written {
        Ok(()) => Command::new(shell).arg("-i").env("ZDOTDIR", &dir).exec(),
        Err(err) => err,
    };
    let _ = fs::remove_dir_all(&dir);
    Err(err)
}

extern "C" fn noop_handler(_: libc::c_int) {}

fn fallback(shell: Option<&OsStr>, argv: &[String]) -> io::Result<Infallible> {
    let reason = match shell {
        Some(shell) => format!("SHELL is {}, not bash or zsh", shell.to_string_lossy()),
        None => "SHELL is not set, using /bin/sh".to_owned(),
    };
    eprintln!("startish: {reason}; the command will not be in shell history");
    let handler = noop_handler as extern "C" fn(libc::c_int) as libc::sighandler_t;
    unsafe {
        libc::signal(libc::SIGINT, handler);
        libc::signal(libc::SIGQUIT, handler);
    }
    if let Err(err) = Command::new(&argv[0]).args(&argv[1..]).status() {
        eprintln!("startish: {}: {err}", argv[0]);
    }
    Err(Command::new(shell.unwrap_or("/bin/sh".as_ref())).exec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Parsed, String> {
        parse_args(args.iter().map(|s| s.to_string()))
    }

    fn run_args(history: &[&str], command: &[&str]) -> Result<Parsed, String> {
        Ok(Parsed::Run(Args {
            history: history.iter().map(|s| s.to_string()).collect(),
            command: command.iter().map(|s| s.to_string()).collect(),
        }))
    }

    #[test]
    fn parses_history_and_command() {
        assert_eq!(
            parse(&[
                "--history",
                "a b",
                "--history=c",
                "--",
                "x",
                "--history",
                "y"
            ]),
            run_args(&["a b", "c"], &["x", "--history", "y"])
        );
    }

    #[test]
    fn command_may_follow_options_without_separator() {
        assert_eq!(parse(&["x", "-y"]), run_args(&[], &["x", "-y"]));
    }

    #[test]
    fn command_may_start_with_dash_after_separator() {
        assert_eq!(parse(&["--", "-x"]), run_args(&[], &["-x"]));
    }

    #[test]
    fn rejects_bad_usage() {
        assert!(parse(&[]).is_err());
        assert!(parse(&["--"]).is_err());
        assert!(parse(&["--history"]).is_err());
        assert!(parse(&["--bogus", "--", "x"]).is_err());
    }

    #[test]
    fn help_and_version() {
        assert_eq!(parse(&["--help"]), Ok(Parsed::Help));
        assert_eq!(parse(&["-V"]), Ok(Parsed::Version));
    }
}
