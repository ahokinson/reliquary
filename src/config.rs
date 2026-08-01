use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

const HEADER: &str = "# reliquary \u{2014} one secret name per line, managed by `reliquary add`/`reliquary remove`\n";

pub fn config_path() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").context("HOME environment variable is not set")?;
    Ok(config_path_in(Path::new(&home)))
}

fn config_path_in(home: &Path) -> PathBuf {
    home.join(".config").join("reliquary").join("config")
}

pub fn load() -> Result<Vec<String>> {
    load_from(&config_path()?)
}

pub fn save(names: &[String]) -> Result<()> {
    save_to(&config_path()?, names)
}

fn load_from(path: &Path) -> Result<Vec<String>> {
    let contents = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    Ok(contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect())
}

fn save_to(path: &Path, names: &[String]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let mut contents = String::from(HEADER);
    for name in names {
        contents.push_str(name);
        contents.push('\n');
    }
    fs::write(path, contents).with_context(|| format!("writing {}", path.display()))
}

/// Must match `^[A-Za-z_][A-Za-z0-9_]*$` — keeps `hook`'s `export NAME=...` safe unquoted.
pub fn validate_name(name: &str) -> Result<()> {
    let mut chars = name.chars();
    let first_ok = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    let rest_ok = chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !first_ok || !rest_ok {
        bail!("\"{name}\" is not a valid env var name (must match [A-Za-z_][A-Za-z0-9_]*)");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_name_accepts_valid_names() {
        assert!(validate_name("GH_TOKEN").is_ok());
        assert!(validate_name("_private").is_ok());
        assert!(validate_name("a1").is_ok());
    }

    #[test]
    fn validate_name_rejects_invalid_names() {
        assert!(validate_name("").is_err());
        assert!(validate_name("1abc").is_err());
        assert!(validate_name("GH-TOKEN").is_err());
        assert!(validate_name("has space").is_err());
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = std::env::temp_dir().join(format!(
            "reliquary-config-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let path = config_path_in(&dir);

        let names = vec!["GH_TOKEN".to_string(), "OPENAI_API_KEY".to_string()];
        save_to(&path, &names).unwrap();
        let loaded = load_from(&path).unwrap();
        assert_eq!(loaded, names);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn load_missing_file_returns_empty() {
        let dir = std::env::temp_dir().join(format!(
            "reliquary-config-test-missing-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let path = config_path_in(&dir);
        assert_eq!(load_from(&path).unwrap(), Vec::<String>::new());
    }
}
