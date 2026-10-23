# startish

START Interactive SHell: run a command as the first thing an interactive shell
does, then leave you at that shell's prompt with the command in its history, so
up-arrow and Enter run it again.

```sh
cargo install --path .
```

## Usage

```sh
startish [--history CMD]... -- COMMAND...
```

```sh
startish --history "wtclaude --no-pull --resume <id> <branch>" -- \
  wtclaude --no-pull <branch> /md-issue-fix <id>
```

- COMMAND is the argv after `--`. Each word is shell-quoted, so it runs exactly
  as given and appears in history as a single command line.
- Each `--history CMD` is a command line pushed into history verbatim.
- History is pushed in the order given, then COMMAND, so up-arrow brings back
  COMMAND first, then the `--history` entries from last to first, then the
  shell's saved history.
- When COMMAND exits, successfully or not, you are at a normal prompt with your
  usual config loaded. Ctrl-C stops COMMAND and leaves you at the prompt;
  Ctrl-Z suspends it as a normal job that `fg` resumes.

The shell is chosen from `$SHELL`. startish writes a startup file to a
temporary location, `exec`s the shell, and the startup file deletes itself once
the shell has read it.

### bash

`bash --rcfile <file> -i`. The rcfile sources the first of `~/.bash_profile`,
`~/.bash_login` and `~/.profile` that exists, as a login shell would, or
`~/.bashrc` if none of them does. It then reads `$HISTFILE` with `history -r`
before `history -s`, because bash skips loading its history file if the
history list is already non-empty when startup finishes. `/etc/profile` is not
sourced.

COMMAND runs from a run-once `PROMPT_COMMAND` hook rather than from the rcfile
itself, because bash only turns on job control after its startup files: run
from the rcfile, COMMAND shares the shell's process group and Ctrl-Z does
nothing. The rcfile saves whatever `PROMPT_COMMAND` the user's config set
(string or bash 5.1+ array, via `declare -p`), and the hook restores it, runs
COMMAND, then runs the restored `PROMPT_COMMAND` once so the first prompt looks
as usual (with `$?` set to COMMAND's status).

### zsh

`zsh -i` with `ZDOTDIR` pointing at a temporary directory:

- its `.zshenv` restores the real `ZDOTDIR` (or unsets it), sources the real
  `.zshenv`, remembers `ZDOTDIR` as that file left it, and points `ZDOTDIR`
  back at the temporary directory;
- its `.zshrc` deletes the temporary directory, restores the remembered
  `ZDOTDIR`, sources the real `.zprofile`, `.zshrc` and `.zlogin`, installs a
  run-once `precmd` hook that pushes the history entries with `print -s`, then
  runs COMMAND. Ctrl-Z works from `.zshrc`, so zsh does not need a hook for
  COMMAND.

History is pushed from `precmd` because zsh loads `$HISTFILE` after `.zshrc`,
which would put the saved history after the pushed entries.

A user `.zshenv` that sets `ZDOTDIR` (e.g. to `~/.config/zsh`) works. A system
`/etc/zshenv` that sets `ZDOTDIR` does not: zsh then never reads startish's
files. The system login files (`/etc/zprofile`, `/etc/zlogin`) are not read,
since the shell is not a login shell.

### Other shells

startish warns that the command will not be in history, runs COMMAND as a
child, then `exec`s `$SHELL` (or `/bin/sh` if `$SHELL` is unset).
