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

On macOS it has to be Apple's `cc`. `security-framework` links `-liconv`,
which exists only as an SDK stub (`libiconv.tbd`), so a GCC earlier on
`$PATH` fails at link time with `library not found for -liconv`. Either
drop it from `$PATH` or point rustc at the right one:

```sh
RUSTFLAGS="-C linker=/usr/bin/cc" cargo build
```

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
  `apple-native-keyring-store` backend, which talks to the *legacy*
  file-based Keychain (`SecKeychainAddGenericPassword` /
  `SecKeychainFindGenericPassword`), not the data-protection keychain.
  That API gives each new item an access control list trusting only the
  binary that created it, identified by its code signature — and
  `SecKeychainAddGenericPassword` takes no access parameter, so the crate
  offers no way to widen it.

  Any package manager that rebuilds the binary therefore invalidates the
  grant on every stored secret. Nix assigns a fresh ad-hoc signature per
  version bump, which produces one Keychain dialog per secret per shell
  startup. "Always Allow" does not help, because the grant is recorded
  against a code identity the next upgrade replaces.

  There is no way to widen those controls after the fact, and the record of
  attempts is worth keeping so nobody repeats them. Editing an existing
  item's controls needs `ACLAuthorizationChangeACL` and `SecACLSetContents`
  consumes it once per entry touched, so retrofitting costs a dialog per
  entry. Worse, `SecKeychainItemSetAccess` returns `errSecAuthFailed`
  (-25293) regardless: from the creating process, with ChangeACL set to
  allow every application, and from inside the item's own partition. It
  cannot be set at creation either. `SecKeychainItemCreateFromContent`
  takes an `initialAccess`, but since macOS 10.12 securityd stamps an
  `ACLAuthorizationPartitionID` entry into the new item whatever that
  argument says. That entry matches on the caller's code signature and is
  consulted *even when the application list already allows everything*,
  it holds its data in the entry's description rather than its application
  list, and for an unsigned build it names a bare cdhash that the next
  rebuild invalidates. `security add-generic-password -A` hits the same
  wall from the other side: it stamps `apple-tool:`, so items it writes
  read back cleanly under `/usr/bin/security` and prompt for everything
  else.

  So the fix is not to fight the access controls but to stop paying them
  twelve times over. `store` keeps every secret in one entry, `ACCOUNT`,
  encoded as length-prefixed records. `hook` reads that entry once and
  looks up each configured name in memory, so a shell startup costs one
  access check no matter how many secrets are configured. That leaves at
  most one dialog per shell on macOS, and none on Linux.

  A dialog is charged per *lookup*, so `store::update` reads once and writes
  back through the handle that read returned, via `security-framework`. Going
  through `keyring`'s `set_password` instead would look the entry up again and
  cost a second one. Only the Decrypt authorization is ever charged: Encrypt is
  open to any application, so modifying and creating are both free, and
  measurement bears that out. Note the dialog appears even for the binary that
  created the entry and is named in its own access controls, which is why
  taking ownership by rewriting the entry buys nothing: an ad-hoc-signed
  binary trips `kSecKeychainPromptInvalid` regardless. That is the same reason
  "Always Allow" never sticks.

  `reliquary repair` moves secrets out of the older one-entry-per-name
  layout, where each secret's name was its own account. It saves the
  consolidated entry before deleting any old one, so an interruption leaves
  both copies.

  When measuring any of this, count dialogs from a *different* binary than
  the one that wrote the entry. A tool reads its own partition without
  prompting, so testing a write with the same tool that made it always
  looks clean.

  When checking any of this by hand, read the item back with something
  other than `/usr/bin/security`. A tool reads its own partition without
  prompting, so testing `security add-generic-password -A` with
  `security find-generic-password` passes regardless of whether the
  partition entry is still there.

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
