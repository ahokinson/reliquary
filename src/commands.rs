use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::cli::Shell;
use crate::config;
use crate::secret::Secret;
use crate::store;

const MARKER_BEGIN: &str = "# >>> reliquary hook >>>";
const MARKER_END: &str = "# <<< reliquary hook <<<";

pub fn init() -> Result<()> {
    let path = config::config_path()?;
    if path.exists() {
        println!("Config already exists at {}", path.display());
    } else {
        config::save(&config::load()?)?;
        println!("Created {}", path.display());
    }

    let Some(shell) = detect_shell() else {
        println!(
            "Could not detect a supported shell from $SHELL (only bash, zsh, and fish are \
             supported). Skipping shell hook setup — you can add it manually later."
        );
        return Ok(());
    };

    let home = std::env::var_os("HOME").context("HOME environment variable is not set")?;
    let home = Path::new(&home);
    let rc_path = rc_path_for(shell, home);
    let hook_line = hook_line_for(shell);

    let existing_contents = fs::read_to_string(&rc_path).unwrap_or_default();
    if existing_contents.contains(MARKER_BEGIN) {
        println!("Already installed in {}", rc_path.display());
        return Ok(());
    }

    println!(
        "This will add the following to {}:\n\n  {MARKER_BEGIN}\n  {hook_line}\n  {MARKER_END}\n",
        rc_path.display()
    );
    if !confirm("Proceed?")? {
        println!("Skipped shell hook setup.");
        return Ok(());
    }

    if let Some(parent) = rc_path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }

    let mut block = String::new();
    if !existing_contents.is_empty() && !existing_contents.ends_with('\n') {
        block.push('\n');
    }
    block.push_str(MARKER_BEGIN);
    block.push('\n');
    block.push_str(hook_line);
    block.push('\n');
    block.push_str(MARKER_END);
    block.push('\n');

    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&rc_path)
        .and_then(|mut file| file.write_all(block.as_bytes()))
        .with_context(|| format!("writing {}", rc_path.display()))?;

    println!(
        "Added hook to {}. Restart your shell (or run: {hook_line}) to activate it.",
        rc_path.display()
    );
    Ok(())
}

fn detect_shell() -> Option<Shell> {
    let shell_path = std::env::var("SHELL").ok()?;
    let name = Path::new(&shell_path).file_name()?.to_str()?;
    match name {
        "bash" => Some(Shell::Bash),
        "zsh" => Some(Shell::Zsh),
        "fish" => Some(Shell::Fish),
        _ => None,
    }
}

fn rc_path_for(shell: Shell, home: &Path) -> PathBuf {
    match shell {
        Shell::Bash => home.join(".bashrc"),
        Shell::Zsh => home.join(".zshrc"),
        Shell::Fish => home.join(".config").join("fish").join("config.fish"),
    }
}

fn hook_line_for(shell: Shell) -> &'static str {
    match shell {
        Shell::Bash => "eval \"$(reliquary hook bash)\"",
        Shell::Zsh => "eval \"$(reliquary hook zsh)\"",
        Shell::Fish => "reliquary hook fish | source",
    }
}

pub fn add(name: &str) -> Result<()> {
    config::validate_name(name)?;

    let mut names = config::load()?;
    let already_configured = names.iter().any(|n| n == name);

    if already_configured && !confirm(&format!("\"{name}\" already exists — overwrite?"))? {
        println!("Cancelled.");
        return Ok(());
    }

    if !io::stdin().is_terminal() {
        bail!("refusing to prompt for a secret value on a non-interactive stdin");
    }
    let value = Secret::new(
        rpassword::prompt_password(format!("Value for {name}: "))
            .context("reading secret value")?,
    );

    store::set(name, &value)?;

    if !already_configured {
        names.push(name.to_string());
        config::save(&names)?;
    }

    println!(
        "Stored \"{name}\". New shells will pick it up automatically; run `eval \"$(reliquary hook bash)\"` \
         (or your shell's equivalent) to pick it up in this one."
    );
    Ok(())
}

pub fn remove(name: &str) -> Result<()> {
    let mut names = config::load()?;
    if !names.iter().any(|n| n == name) {
        println!("\"{name}\" is not configured.");
        return Ok(());
    }

    if !confirm(&format!("Remove \"{name}\"?"))? {
        println!("Cancelled.");
        return Ok(());
    }

    store::delete(name)?;
    names.retain(|n| n != name);
    config::save(&names)?;
    println!("Removed \"{name}\".");
    Ok(())
}

pub fn list() -> Result<()> {
    let names = config::load()?;
    if names.is_empty() {
        println!("No secrets configured yet. Run `reliquary add <NAME>` to add one.");
        return Ok(());
    }
    for name in &names {
        let status = match store::get(name)? {
            Some(_) => "present",
            None => "MISSING from keyring",
        };
        println!("{name}: {status}");
    }
    Ok(())
}

fn confirm(prompt: &str) -> Result<bool> {
    if !io::stdin().is_terminal() {
        bail!("refusing to prompt for confirmation on a non-interactive stdin");
    }
    print!("{prompt} [y/N] ");
    io::stdout().flush().ok();
    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .context("reading confirmation")?;
    let answer = line.trim().to_ascii_lowercase();
    Ok(answer == "y" || answer == "yes")
}
