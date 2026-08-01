# reliquary

A reliquary holds something valuable and keeps it out of sight. This one
holds your API tokens and keys in your OS's native keyring, and loads
them into your shell's environment on startup, no vault file or master
password needed. Secrets live in your platform's own store: Keychain on
macOS, KWallet/Secret Service on KDE, the same place your browser and
system already trust.

## Philosophy

Secrets shouldn't exist as files. A `.env`, a hardcoded `export FOO=bar`
in `.zshrc`, a config with real values in it; all of it is plaintext
sitting on disk and one `cat` away from anyone with read access to your
home directory. Your OS already has a real secure store built for this.
Keep secrets there instead.

## Install

**Nix** (flake, [ahokinson/flake](https://github.com/ahokinson/flake)):

```sh
nix profile install github:ahokinson/flake#reliquary
# or try it without installing:
nix run github:ahokinson/flake#reliquary -- init
```

**Homebrew** (tap, [ahokinson/homebrew-tap](https://github.com/ahokinson/homebrew-tap)):

```sh
brew install ahokinson/tap/reliquary
```

Building from source is covered in [CONTRIBUTING.md](CONTRIBUTING.md).

## Quick start

```sh
reliquary init          # one-time setup: creates the config, offers to hook your shell
reliquary add GH_TOKEN   # prompts for the value, hidden input
```

Open a new terminal and `$GH_TOKEN` is set.

## Commands

| Command                 | Does                                                                               |
| ----------------------- | ---------------------------------------------------------------------------------- |
| `reliquary init`        | One-time setup: creates `~/.config/reliquary/config`, offers to add the shell hook |
| `reliquary add NAME`    | Prompts for a secret's value (hidden input) and stores it, also how you update one |
| `reliquary set NAME`    | Alias for `add`                                                                    |
| `reliquary list`        | Shows configured secrets and whether each is present in the keyring                |
| `reliquary remove NAME` | Deletes a secret from both the keyring and the config                              |

A secret's name doubles as its keyring key *and* the environment variable
it's exported as. `reliquary add GH_TOKEN` always produces `$GH_TOKEN`.
Names must look like a real environment variable
(`[A-Za-z_][A-Za-z0-9_]*`).

New secrets take effect in *new* shells automatically. To pick one up in
your current shell, re-run the line `reliquary init` added to your shell's
rc file directly.

The shell hook never breaks your shell: a locked keyring, a missing secret,
or a dead D-Bus session just prints a warning and moves on.

For how the shell hook actually works, platform-specific keyring notes, and
building from source, see [CONTRIBUTING.md](CONTRIBUTING.md).
