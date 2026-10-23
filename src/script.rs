use std::fmt::Write;

pub fn quote(word: &str) -> String {
    let plain = |b: u8| b.is_ascii_alphanumeric() || b"_@%+=:,./-".contains(&b);
    if !word.is_empty() && !word.starts_with('=') && word.bytes().all(plain) {
        word.to_owned()
    } else {
        format!("'{}'", word.replace('\'', r"'\''"))
    }
}

pub fn command_line(argv: &[String]) -> String {
    argv.iter().map(|w| quote(w)).collect::<Vec<_>>().join(" ")
}

pub fn zshenv(tmpdir: &str, zdotdir: Option<&str>) -> String {
    let restore = match zdotdir {
        Some(dir) => format!("ZDOTDIR={}", quote(dir)),
        None => "unset ZDOTDIR".to_owned(),
    };
    format!(
        "{restore}\n\
         if [[ -r ${{ZDOTDIR:-$HOME}}/.zshenv ]]; then . \"${{ZDOTDIR:-$HOME}}/.zshenv\"; fi\n\
         if (( ${{+ZDOTDIR}} )); then __startish_zdotdir=$ZDOTDIR; fi\n\
         ZDOTDIR={}\n",
        quote(tmpdir)
    )
}

pub struct Startup<'a> {
    pub history: &'a [String],
    pub command: &'a str,
}

impl Startup<'_> {
    fn entries(&self) -> impl Iterator<Item = &str> {
        self.history
            .iter()
            .map(String::as_str)
            .chain([self.command])
    }

    pub fn bash_rcfile(&self, rcfile: &str) -> String {
        let mut s = format!("command rm -f -- {}\n", quote(rcfile));
        s.push_str(
            "if [ -f ~/.bash_profile ]; then . ~/.bash_profile\n\
             elif [ -f ~/.bash_login ]; then . ~/.bash_login\n\
             elif [ -f ~/.profile ]; then . ~/.profile\n\
             elif [ -f ~/.bashrc ]; then . ~/.bashrc\n\
             fi\n\
             if [ -n \"${HISTFILE-}\" ] && [ -r \"$HISTFILE\" ]; then history -r; fi\n",
        );
        for entry in self.entries() {
            writeln!(s, "history -s -- {}", quote(entry)).unwrap();
        }
        s.push_str(&self.bash_run_hook());
        s
    }

    fn bash_run_hook(&self) -> String {
        let hook = format!(
            "unset PROMPT_COMMAND; eval \"$__startish_saved\"; unset __startish_saved\n\
             {}\n\
             for __startish_c in ${{PROMPT_COMMAND[@]+\"${{PROMPT_COMMAND[@]}}\"}}; do eval \"$__startish_c\"; done\n\
             unset __startish_c",
            self.command
        );
        format!(
            "__startish_saved=$(declare -p PROMPT_COMMAND 2>/dev/null)\n\
             unset PROMPT_COMMAND\n\
             PROMPT_COMMAND={}\n",
            quote(&hook)
        )
    }

    pub fn zshrc(&self, tmpdir: &str) -> String {
        let mut s = format!("command rm -rf -- {}\n", quote(tmpdir));
        s.push_str(
            "if (( ${+__startish_zdotdir} )); then ZDOTDIR=$__startish_zdotdir; else unset ZDOTDIR; fi\n\
             unset __startish_zdotdir\n",
        );
        for file in [".zprofile", ".zshrc", ".zlogin"] {
            writeln!(
                s,
                "if [[ -r ${{ZDOTDIR:-$HOME}}/{file} ]]; then . \"${{ZDOTDIR:-$HOME}}/{file}\"; fi"
            )
            .unwrap();
        }
        s.push_str(
            "__startish_history() {\n  \
             precmd_functions=(${precmd_functions:#__startish_history})\n  \
             unfunction __startish_history\n",
        );
        for entry in self.entries() {
            writeln!(s, "  print -sr -- {}", quote(entry)).unwrap();
        }
        s.push_str("}\nprecmd_functions=(__startish_history $precmd_functions)\n");
        writeln!(s, "{}", self.command).unwrap();
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(ws: &[&str]) -> Vec<String> {
        ws.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn plain_words_are_not_quoted() {
        assert_eq!(quote("wtclaude"), "wtclaude");
        assert_eq!(quote("--no-pull"), "--no-pull");
        assert_eq!(quote("/md-issue-fix"), "/md-issue-fix");
        assert_eq!(quote("a=b"), "a=b");
    }

    #[test]
    fn special_words_are_single_quoted() {
        assert_eq!(quote(""), "''");
        assert_eq!(quote("a b"), "'a b'");
        assert_eq!(quote("$HOME"), "'$HOME'");
        assert_eq!(quote("~"), "'~'");
        assert_eq!(quote("=ls"), "'=ls'");
        assert_eq!(quote("!!"), "'!!'");
        assert_eq!(quote("it's"), r"'it'\''s'");
        assert_eq!(quote("'"), r"''\'''");
    }

    #[test]
    fn command_line_joins_quoted_words() {
        let argv = words(&["wtclaude", "--no-pull", "my branch", "it's"]);
        assert_eq!(
            command_line(&argv),
            r"wtclaude --no-pull 'my branch' 'it'\''s'"
        );
    }

    #[test]
    fn quoted_words_round_trip_through_sh() {
        let argv = words(&["a b", "it's", "$X", "`id`", "\"", "\\", "", "\n", "*"]);
        let script = format!("printf '%s\\0' {}", command_line(&argv));
        let out = std::process::Command::new("sh")
            .args(["-c", &script])
            .output()
            .unwrap();
        let got: Vec<String> = String::from_utf8(out.stdout)
            .unwrap()
            .split_terminator('\0')
            .map(String::from)
            .collect();
        assert_eq!(got, argv);
    }

    #[test]
    fn bash_rcfile_pushes_history_then_installs_run_hook() {
        let history = words(&["echo one", "echo 'two'"]);
        let startup = Startup {
            history: &history,
            command: "make test",
        };
        let rc = startup.bash_rcfile("/tmp/startish-x.bash");
        assert!(rc.starts_with("command rm -f -- /tmp/startish-x.bash\n"));
        assert!(rc.contains(
            "history -r; fi\n\
             history -s -- 'echo one'\n\
             history -s -- 'echo '\\''two'\\'''\n\
             history -s -- 'make test'\n"
        ));
        assert!(rc.ends_with(&startup.bash_run_hook()));
        let profile = rc.find(". ~/.bash_profile").unwrap();
        assert!(profile < rc.find("history -r").unwrap());
    }

    fn run_bash_hook(setup: &str) -> String {
        let startup = Startup {
            history: &[],
            command: "echo ran; (exit 4)",
        };
        let script = format!(
            "{setup}\n{}eval \"$PROMPT_COMMAND\"\necho ---\n\
             declare -p PROMPT_COMMAND 2>/dev/null\n\
             compgen -v __startish; compgen -A function __startish; true",
            startup.bash_run_hook()
        );
        let out = std::process::Command::new("bash")
            .args(["-c", &script])
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap()
    }

    #[test]
    fn bash_hook_runs_command_then_restores_string_prompt_command() {
        assert_eq!(
            run_bash_hook("PROMPT_COMMAND='echo user-$?'"),
            "ran\nuser-4\n---\ndeclare -- PROMPT_COMMAND=\"echo user-\\$?\"\n"
        );
    }

    #[test]
    fn bash_hook_restores_array_prompt_command() {
        assert_eq!(
            run_bash_hook("PROMPT_COMMAND=('echo a1' 'echo a2')"),
            "ran\na1\na2\n---\ndeclare -a PROMPT_COMMAND=([0]=\"echo a1\" [1]=\"echo a2\")\n"
        );
    }

    #[test]
    fn bash_hook_leaves_prompt_command_unset() {
        assert_eq!(run_bash_hook("unset PROMPT_COMMAND"), "ran\n---\n");
    }

    #[test]
    fn zshenv_restores_real_zdotdir_and_returns_to_tmpdir() {
        let env = zshenv("/tmp/s", Some("/home/u/.config/zsh"));
        assert!(env.starts_with("ZDOTDIR=/home/u/.config/zsh\n"));
        assert!(env.ends_with("ZDOTDIR=/tmp/s\n"));
        assert!(zshenv("/tmp/s", None).starts_with("unset ZDOTDIR\n"));
    }

    #[test]
    fn zshrc_sources_user_files_then_runs_command() {
        let history = words(&["echo one"]);
        let startup = Startup {
            history: &history,
            command: "make test",
        };
        let rc = startup.zshrc("/tmp/s");
        assert!(rc.starts_with("command rm -rf -- /tmp/s\n"));
        let pos = |needle: &str| rc.find(needle).unwrap();
        assert!(pos("/.zprofile") < pos("/.zshrc"));
        assert!(pos("/.zshrc") < pos("/.zlogin"));
        assert!(pos("print -sr -- 'echo one'") < pos("print -sr -- 'make test'"));
        assert!(rc.ends_with("$precmd_functions)\nmake test\n"));
    }
}
