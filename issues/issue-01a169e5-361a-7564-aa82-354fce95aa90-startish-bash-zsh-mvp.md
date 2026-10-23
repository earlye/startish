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
  Ctrl-C and Ctrl-Z. The belief is that job control is set up before startup
  files are read, but this needs testing in a real tmux window under both
  shells before anything else depends on it.

## Open questions

- Language: a shell script (no install beyond copying it onto `PATH`) or a
  compiled binary (like `wtclaude`, via `cargo install`).
- Whether `wtclaude` should someday print or record the session id it just
  ran, so a caller can pass the exact `--resume <id>` to `--history` instead
  of a `SESSION_ID` placeholder. That work belongs in `wtclaude`, not here.
