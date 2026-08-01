use zeroize::Zeroize;

use crate::cli::Shell;
use crate::{config, store};

/// Prints shell code to stdout for `eval`/`source` at shell startup. Stdout
/// carries only executable lines; everything else goes to stderr, and this
/// never exits non-zero — a broken keyring must never break shell startup.
pub fn run(shell: Shell) {
    let names = match config::load() {
        Ok(names) => names,
        Err(e) => {
            eprintln!("reliquary: warning: {e:#}");
            eprintln!("reliquary: run `reliquary init` to get started");
            return;
        }
    };

    for name in &names {
        match store::get(name) {
            Ok(Some(value)) => print_export(shell, name, &value),
            Ok(None) => eprintln!(
                "reliquary: warning: secret \"{name}\" is configured but not found in the OS keyring (run: reliquary add {name})"
            ),
            Err(e) => eprintln!("reliquary: warning: {e:#}"),
        }
    }
}

fn print_export(shell: Shell, name: &str, value: &str) {
    match shell {
        Shell::Bash | Shell::Zsh => match shlex::try_quote(value) {
            Ok(mut quoted) => {
                println!("export {name}={quoted}");
                if let std::borrow::Cow::Owned(s) = &mut quoted {
                    s.zeroize();
                }
            }
            Err(_) => eprintln!(
                "reliquary: warning: secret \"{name}\" contains a NUL byte and can't be safely exported"
            ),
        },
        Shell::Fish => {
            let mut quoted = fish_quote(value);
            println!("set -gx {name} {quoted}");
            quoted.zeroize();
        }
    }
}

/// fish's single-quote escaping only needs to handle `\` and `'`, unlike POSIX.
fn fish_quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('\'');
    for c in value.chars() {
        if c == '\\' || c == '\'' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('\'');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fish_quote_escapes_backslash_and_single_quote() {
        assert_eq!(fish_quote("plain"), "'plain'");
        assert_eq!(fish_quote("it's"), "'it\\'s'");
        assert_eq!(fish_quote(r"back\slash"), r"'back\\slash'");
    }
}
