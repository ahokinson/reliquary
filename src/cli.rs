use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(
    name = "reliquary",
    about = "Store secrets in your OS keyring and load them into your shell env on startup"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Create the config file and offer to hook into your shell's rc file
    Init,
    /// Add or update a secret (prompts for the value, hidden input)
    #[command(alias = "set")]
    Add { name: String },
    /// Remove a secret from the keyring and the config
    Remove { name: String },
    /// List configured secrets and whether they're present in the keyring
    List,
    /// Print shell code that exports configured secrets as env vars
    Hook { shell: Shell },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
}
