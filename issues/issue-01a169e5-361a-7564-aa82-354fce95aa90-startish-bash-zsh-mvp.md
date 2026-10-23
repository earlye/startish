# startish: run a command, then leave an interactive shell with it in history

## Context

`startish` (START Interactive SHell) runs a command as the first thing an
interactive shell does, then leaves you at that shell's prompt with the
command in its history, so up-arrow and Enter run it again.

No common shell has a flag for this. `bash -c cmd` is not interactive, and
`bash -i -c cmd` exits when `cmd` does. Each shell has its own workaround, and
every caller that wants the behaviour ends up copying shell-specific code.

The motivating case is earlye/skills: `/md-issue tmux` and `/initiative tmux`
open a tmux window whose command is `wtclaude <branch> [/md-issue-fix <id>]`.
The window closed when `wtclaude` exited, and Early restarts `wtclaude` often
to pick up new builds. The interim fix (earlye/skills branch
`earlye/ebb118fbcc0b/tmux-window-outlives-wtclaude`) prints restart commands
when `wtclaude` exits and then runs `exec "$SHELL"`. That works in any shell,
but the restart lines have to be copied by hand, and getting them to print
correctly takes two layers of `printf %q`. With `startish`, the window command
would become:

```sh
startish --history "wtclaude --no-pull --resume <id> <branch>" -- \
  wtclaude --no-pull <branch> /md-issue-fix <id>
```

## Requirements

- **bash and zsh are supported from the start.** Detect the shell from
  `$SHELL`.
- `startish [--history CMD]... -- COMMAND...`. Push each `--history` entry into
  the shell's history, then push COMMAND, so that COMMAND is the first thing
  up-arrow brings back and the `--history` entries come after it. Then run
  COMMAND in the interactive shell.
- When COMMAND exits, whether it succeeded or failed, the user is at a normal
  prompt with their usual config loaded.
- For an unsupported shell, fall back to running COMMAND, then
  `exec "$SHELL"`, with a warning that the command is not in history.

## Known mechanics

These come from the discussion that produced this issue. None of them has been
tested here.

- **bash:** `bash --rcfile <file>`, where the file sources the user's config,
  runs `history -s` for each entry, then runs COMMAND.
  - `--rcfile` is ignored by login shells, so passing `-l` doesn't help. To
    load the login environment, the rcfile should source `~/.bash_profile`
    (which normally sources `~/.bashrc`), not `~/.bashrc` directly.
- **zsh:** there is no `--rcfile`. Point `ZDOTDIR` at a temporary directory
  whose `.zshrc` does three things:
  - resets `ZDOTDIR` to the user's real value
  - sources the real `.zshrc` (and probably `.zshenv` and `.zprofile`, since
    those are also read from `ZDOTDIR`)
  - runs `print -s` for each entry, then runs COMMAND
- **fish**, if it is ever added, has `--init-command` natively.
- **Use a temporary file, not `<(…)`.** Process substitution is expanded by
  whichever shell runs the `startish` command line (e.g. tmux's
  `default-shell -c`), not by `$SHELL`. Remove the temporary files once the
  shell has read them.
- **Unverified:** whether a full-screen program (`claude`) started from inside
  an rcfile or `.zshrc` gets the terminal properly, including job control,
  Ctrl-C and Ctrl-Z. Expectation: Ctrl-C works (the foreground process group
  is set before startup files run); Ctrl-Z is the uncertain one. This is an
  acceptance check, not a blocker — see Decisions.

## Decisions

- **Language: Rust**, installed with `cargo install`, for consistency with
  `wtclaude`. The binary writes the per-shell startup file to a temporary
  location and `exec`s the shell.
- **COMMAND runs at the end of the generated startup file.** If Ctrl-Z (or
  anything else) misbehaves in real use, switch to a run-once first-prompt hook
  (bash `PROMPT_COMMAND`, zsh `precmd`), which runs after startup completes.
  Keep the generator structured so that switch is small.
- **Tests:** pseudo-terminal integration tests (e.g. the `rexpect` crate) under
  bash and zsh covering history order (COMMAND first on up-arrow, then the
  `--history` entries) and landing at a prompt after both a succeeding and a
  failing COMMAND. Ctrl-C / Ctrl-Z in a real tmux window are a manual checklist
  in the PR description.

## Out of scope

- `wtclaude` printing or recording the session id it ran, so callers can pass
  an exact `--resume <id>`. That work belongs in `wtclaude`.

## Grill Log

### 2026-10-23

- Q: Shell script or compiled binary? — A: Rust binary (`cargo install`), for
  consistency with `wtclaude`'s ecosystem. A POSIX `sh` script was considered
  and rejected despite being simpler to install.
- Q: Spike job control (Ctrl-C/Ctrl-Z from a startup file) before building, or
  treat it as an acceptance check? — A: Acceptance check. Run COMMAND from the
  startup file; fall back to a first-prompt hook if it turns out badly, and
  iterate. Automated pty tests for history order and post-exit prompt; manual
  tmux checklist for Ctrl-C/Ctrl-Z.
