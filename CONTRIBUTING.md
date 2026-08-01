# Contributing

## Building from source

```sh
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt
```

Needs a C linker (`cc`/`gcc`) available for `cargo build` to link the
binary. Most systems already have one; on NixOS it isn't there by default
and needs adding to your package list.

Install a locally-built binary with `cargo install --path .`.

## How the shell hook works

A subprocess can't set environment variables in its parent shell — that's
just how process environments work. So instead, `reliquary hook <shell>`
prints shell code to stdout, and your shell's startup file runs that code
directly. This is the same idiom `direnv`, `atuin`, and `ssh-agent` use.

`reliquary init` offers to add one of these lines for you:

```sh
# ~/.bashrc
eval "$(reliquary hook bash)"

# ~/.zshrc
eval "$(reliquary hook zsh)"

# ~/.config/fish/config.fish
reliquary hook fish | source
```

It only needs to run once per shell startup (not on every command), and
`reliquary init` detects an existing hook line and won't duplicate it.

`hook`'s contract: stdout only ever carries `export NAME=value` /
`set -gx NAME value` lines, every diagnostic goes to stderr, and it always
exits 0. A missing secret or an unreachable keyring backend just prints a
warning — breaking someone's shell startup is worse than a missing env var.

Secret values are quoted before being printed, since they're arbitrary
user input that may contain quotes, `$`, backticks, or newlines:

- bash/zsh use the `shlex` crate (`try_quote`): POSIX quoting has enough
  sharp edges (backslash handling varies by context, `$'...'` ANSI-C
  quoting, etc.) that hand-rolling it would be a real mistake.
- fish is hand-rolled, deliberately: fish's own single-quote escaping rules
  are simpler than POSIX's (only `\` and `'` are ever special), so the
  hand-rolled version is complete and correct on its own. See
  `fish_quote` in `src/hook.rs`.

## Dependencies

Keep the list short. Hand-roll anything simple (path joining, parsing a
flat text file, a y/n prompt) and only reach for a crate when the logic
is genuinely tricky or security-sensitive: shell quoting, keyring access,
wiping memory correctly.

| Crate       | Why                                                                 |
| ----------- | -------------------------------------------------------------------- |
| `clap`      | CLI parsing. The one "heavier" dependency here, but about as widely audited as it gets (ripgrep, fd, and bat all use it) |
| `keyring`   | OS keyring access (Keychain / Secret Service), the whole point of the tool |
| `rpassword` | Hidden-input password prompt, the same one `cargo` itself uses     |
| `shlex`     | POSIX-safe shell quoting, see below                                 |
| `zeroize`   | Wipes secret buffers on drop in a way the compiler can't optimize away |
| `libc`      | `mlock`/`munlock` and disabling core dumps (see Memory handling below) |
| `anyhow`    | Error-context plumbing, no dependencies of its own                   |

No `serde`/`toml`: config is just a flat list of names, one per line,
plain text is simpler. No `directories`/`dirs`: only Linux and macOS are
targeted, and both use `~/.config` by convention. No `dialoguer`, see
`rpassword` above.

## Memory handling

Secrets never touch disk (no config value, no cache file), but the
process holds them in memory for a moment while it reads and prints them.
The value read back from the keyring and the value typed into `add`'s
prompt are wrapped in `Secret` (`src/secret.rs`), which `mlock`s the pages
so they can't be swapped to disk and wipes them the moment they're
dropped. The shell-quoted copy `hook` builds right before printing it
gets wiped the same way. Core dumps are off for the whole process
(`RLIMIT_CORE` set to 0 at startup), so a crash can't leave a memory
snapshot with secrets in it lying around on disk.

None of that stops a privileged process from reading our memory live via
`ptrace` or `/proc/<pid>/mem`, which is true of any CLI tool. The
plaintext also passes briefly through `rpassword`'s and `keyring`'s own
internals before we get a copy, which is outside our control too. And
`mlock` is best-effort: if it fails, say under a container's tight
`RLIMIT_MEMLOCK`, that's not treated as fatal, same as a locked keyring
not blocking shell startup.

## Platform notes

- **Linux / KDE**: uses the Secret Service D-Bus API (via the `keyring`
  crate's pure-Rust `zbus` backend, no system `libdbus` needed to build),
  which on modern Plasma is served directly by KWallet. Secrets written
  this way and secrets written via the legacy KWallet GUI API can land in
  different backing stores; a secret added with `reliquary` may not show
  up in KWalletManager, and vice versa. On a normal graphical login,
  `pam_kwallet` unlocks the wallet automatically, so there's no unlock
  prompt in the common case.
- **macOS**: uses Keychain Services via the `keyring` crate's
  `apple-native-keyring-store` backend. Untested on real hardware so far
  (developed on Linux), so this relies on the crate's cross-platform API
  guarantee (identical `Entry` calls on every platform) rather than direct
  verification.

## Testing the shell hook end-to-end

Unit tests cover config parsing and fish quoting, but the hook mechanism
itself is best verified live:

```sh
cargo run -- add GH_TOKEN            # store a test value
eval "$(cargo run -q -- hook bash)"  # or the zsh/fish equivalent
echo "$GH_TOKEN"                     # should match what you entered
```

To test `init` without touching your real shell config, point `HOME` at a
scratch directory: `HOME=$(mktemp -d) cargo run -- init`.
