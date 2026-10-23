use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use rexpect::session::{PtySession, spawn_command};
use tempfile::TempDir;

const PROMPT: &str = "PROMPT> ";
const UP: &str = "\x1b[A";

#[derive(Clone, Copy)]
enum Shell {
    Bash,
    Zsh,
}

impl Shell {
    fn name(self) -> &'static str {
        match self {
            Shell::Bash => "bash",
            Shell::Zsh => "zsh",
        }
    }

    fn path(self) -> Option<PathBuf> {
        let found = env::split_paths(&env::var_os("PATH")?)
            .map(|dir| dir.join(self.name()))
            .find(|p| p.is_file());
        if found.is_none() {
            eprintln!("skipping: {} is not on PATH", self.name());
        }
        found
    }

    fn home(self) -> TempDir {
        let home = TempDir::new_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
        let write = |rel: &str, text: &str| {
            let path = home.path().join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        };
        match self {
            Shell::Bash => {
                write(".bash_profile", ". ~/.bashrc\n");
                write(
                    ".bashrc",
                    &format!("PS1='{PROMPT}'\nHISTFILE=~/.bash_history\n"),
                );
                write(".bash_history", "echo OLD-$((5+5))\n");
            }
            Shell::Zsh => {
                write(".zshenv", "ZDOTDIR=$HOME/zdot\n");
                write(
                    "zdot/.zshrc",
                    &format!(
                        "PS1='{PROMPT}'\nunsetopt PROMPT_SP\nbindkey -e\n\
                         HISTFILE=$ZDOTDIR/.zsh_history\nHISTSIZE=100\nSAVEHIST=100\n"
                    ),
                );
                write("zdot/.zsh_history", "echo OLD-$((5+5))\n");
            }
        }
        home
    }
}

fn startish(shell: &Path, home: &Path, args: &[&str]) -> PtySession {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_startish"));
    cmd.args(args)
        .env_clear()
        .env("HOME", home)
        .env("SHELL", shell)
        .env("PATH", env::var_os("PATH").unwrap())
        .env("TERM", "dumb")
        .env("TMPDIR", home);
    spawn_command(cmd, Some(10_000)).unwrap()
}

fn exit(mut p: PtySession) {
    p.send_line("exit").unwrap();
    p.exp_eof().unwrap();
}

fn up_arrow_order(shell: Shell) {
    let Some(path) = shell.path() else { return };
    let args = [
        "--history",
        "echo H1-$((1+1))",
        "--history",
        "echo H2-$((2+1))",
        "--",
        "sh",
        "-c",
        "echo X$((40+2))",
    ];
    for (presses, expected) in [(1, "X42"), (2, "H2-3"), (3, "H1-2"), (4, "OLD-10")] {
        let home = shell.home();
        let mut p = startish(&path, home.path(), &args);
        p.exp_string("X42").unwrap();
        p.exp_string(PROMPT).unwrap();
        p.send(&UP.repeat(presses)).unwrap();
        p.send("\r").unwrap();
        p.flush().unwrap();
        p.exp_string(expected)
            .unwrap_or_else(|e| panic!("{}: {presses} x up: {e}", shell.name()));
        exit(p);
    }
}

fn lands_at_prompt(shell: Shell, script: &str, marker: &str) {
    let Some(path) = shell.path() else { return };
    let home = shell.home();
    let mut p = startish(&path, home.path(), &["--", "sh", "-c", script]);
    p.exp_string(marker).unwrap();
    p.exp_string(PROMPT).unwrap();
    p.send_line("echo alive-$((2+2))").unwrap();
    p.exp_string("alive-4").unwrap();
    p.exp_string(PROMPT).unwrap();
    let leftovers: Vec<_> = fs::read_dir(home.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("startish-"))
        .collect();
    assert!(leftovers.is_empty(), "temp files left: {leftovers:?}");
    exit(p);
}

#[test]
fn bash_up_arrow_brings_back_command_then_history_entries() {
    up_arrow_order(Shell::Bash);
}

#[test]
fn zsh_up_arrow_brings_back_command_then_history_entries() {
    up_arrow_order(Shell::Zsh);
}

#[test]
fn bash_prompt_after_success() {
    lands_at_prompt(Shell::Bash, "echo ok-$((1+1)); exit 0", "ok-2");
}

#[test]
fn bash_prompt_after_failure() {
    lands_at_prompt(Shell::Bash, "echo fail-$((1+2)); exit 3", "fail-3");
}

#[test]
fn zsh_prompt_after_success() {
    lands_at_prompt(Shell::Zsh, "echo ok-$((1+1)); exit 0", "ok-2");
}

#[test]
fn zsh_prompt_after_failure() {
    lands_at_prompt(Shell::Zsh, "echo fail-$((1+2)); exit 3", "fail-3");
}

fn ctrl_z_suspends_and_fg_resumes(shell: Shell) {
    let Some(path) = shell.path() else { return };
    let home = shell.home();
    let script = "echo started; sleep 2; echo resumed-$((1+1))";
    let mut p = startish(&path, home.path(), &["--", "sh", "-c", script]);
    p.exp_string("started").unwrap();
    p.send_control('z').unwrap();
    p.exp_string(PROMPT).unwrap();
    p.send_line("jobs; echo jobs-$((3+3))").unwrap();
    p.exp_string("echo started").unwrap();
    p.exp_string("jobs-6").unwrap();
    p.exp_string(PROMPT).unwrap();
    p.send_line("fg").unwrap();
    p.exp_string("resumed-2").unwrap();
    p.exp_string(PROMPT).unwrap();
    exit(p);
}

fn ctrl_c_returns_to_prompt(shell: Shell) {
    let Some(path) = shell.path() else { return };
    let home = shell.home();
    let script = "echo started; sleep 30; echo finished";
    let mut p = startish(&path, home.path(), &["--", "sh", "-c", script]);
    p.exp_string("started").unwrap();
    p.send_control('c').unwrap();
    p.exp_string(PROMPT).unwrap();
    p.send_line("echo alive-$((2+2))").unwrap();
    let (before, _) = p.exp_regex("alive-4").unwrap();
    assert!(!before.contains("finished"));
    exit(p);
}

#[test]
fn bash_ctrl_z_suspends_command() {
    ctrl_z_suspends_and_fg_resumes(Shell::Bash);
}

#[test]
fn zsh_ctrl_z_suspends_command() {
    ctrl_z_suspends_and_fg_resumes(Shell::Zsh);
}

#[test]
fn bash_ctrl_c_interrupts_command() {
    ctrl_c_returns_to_prompt(Shell::Bash);
}

#[test]
fn zsh_ctrl_c_interrupts_command() {
    ctrl_c_returns_to_prompt(Shell::Zsh);
}
