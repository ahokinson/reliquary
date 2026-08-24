use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use keyring::Entry;

use crate::secret::Secret;

const SERVICE: &str = "reliquary";

/// Every secret lives in this one entry. A keyring charges its access controls
/// per entry, so on macOS N entries means N Keychain dialogs on every shell
/// startup; one entry means one.
const ACCOUNT: &str = "secrets";

pub type Secrets = BTreeMap<String, Secret>;

fn entry(account: &str) -> Result<Entry> {
    Entry::new(SERVICE, account)
        .with_context(|| format!("opening the keyring entry for \"{account}\""))
}

/// An empty map means nothing has been stored yet, not an error.
pub fn load() -> Result<Secrets> {
    let (blob, _) = platform::open()?;
    match blob {
        Some(blob) => decode(&blob),
        None => Ok(Secrets::new()),
    }
}

/// Reads the entry, applies a change, and writes it back through the handle the
/// read produced. Looking the entry up a second time in order to write it would
/// decrypt it a second time, and on macOS every decrypt is a Keychain dialog.
pub fn update(change: impl FnOnce(&mut Secrets) -> Result<()>) -> Result<()> {
    let (blob, handle) = platform::open()?;
    let mut secrets = match blob {
        Some(blob) => decode(&blob)?,
        None => Secrets::new(),
    };

    change(&mut secrets)?;

    if secrets.is_empty() {
        return platform::remove(handle);
    }
    platform::write(handle, &Secret::new(encode(&secrets)))
}

pub fn set(name: &str, value: &str) -> Result<()> {
    let name = name.to_string();
    let value = Secret::new(value.to_string());
    update(move |secrets| {
        secrets.insert(name, value);
        Ok(())
    })
}

/// Treats "already absent" as success.
pub fn delete(name: &str) -> Result<()> {
    update(|secrets| {
        secrets.remove(name);
        Ok(())
    })
}

/// The old layout gave each secret its own entry, keyed by the secret's name,
/// so callers migrating from it pass a name as the account.
pub fn read(account: &str) -> Result<Option<Secret>> {
    match entry(account)?.get_password() {
        Ok(blob) => Ok(Some(Secret::new(blob))),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(keyring::Error::Ambiguous(_)) => bail!(
            "multiple keyring entries match \"{SERVICE}\" — resolve the duplicate via your keyring GUI"
        ),
        Err(e) => Err(e).with_context(|| format!("reading \"{account}\" from the keyring")),
    }
}

pub fn clear(account: &str) -> Result<()> {
    match entry(account)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e).with_context(|| format!("deleting \"{account}\" from the keyring")),
    }
}

/// Writing through a handle from an earlier read is a macOS concern: its
/// keyring charges an authorization per lookup. Elsewhere `keyring`'s own write
/// is fine, so the handle is nothing.
#[cfg(not(target_os = "macos"))]
mod platform {
    use anyhow::{Context, Result};

    use super::{ACCOUNT, Secret, clear, entry, read};

    pub struct Handle;

    pub fn open() -> Result<(Option<Secret>, Handle)> {
        Ok((read(ACCOUNT)?, Handle))
    }

    pub fn write(_handle: Handle, blob: &str) -> Result<()> {
        entry(ACCOUNT)?
            .set_password(blob)
            .context("writing to the keyring")
    }

    pub fn remove(_handle: Handle) -> Result<()> {
        clear(ACCOUNT)
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use anyhow::{Context, Result};
    use security_framework::os::macos::keychain_item::SecKeychainItem;
    use security_framework::os::macos::passwords::find_generic_password;

    use super::{ACCOUNT, SERVICE, Secret, clear, entry};

    const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;

    /// `None` when the entry does not exist yet, so there is nothing to modify.
    pub struct Handle(Option<SecKeychainItem>);

    pub fn open() -> Result<(Option<Secret>, Handle)> {
        match find_generic_password(None, SERVICE, ACCOUNT) {
            Ok((password, item)) => {
                let blob = String::from_utf8(password.to_vec())
                    .context("keyring contents are not valid UTF-8")?;
                Ok((Some(Secret::new(blob)), Handle(Some(item))))
            }
            Err(e) if e.code() == ERR_SEC_ITEM_NOT_FOUND => Ok((None, Handle(None))),
            Err(e) => Err(e).context("reading the keyring"),
        }
    }

    /// Modifying through the handle needs only the Encrypt authorization, which
    /// costs no dialog. Creating an entry costs none either, so the missing-item
    /// case can go through `keyring`.
    pub fn write(handle: Handle, blob: &str) -> Result<()> {
        match handle.0 {
            Some(mut item) => item
                .set_password(blob.as_bytes())
                .context("writing to the keyring"),
            None => {
                entry(ACCOUNT)?
                    .set_password(blob)
                    .context("creating the keyring entry")?;
                Ok(())
            }
        }
    }

    pub fn remove(handle: Handle) -> Result<()> {
        match handle.0 {
            Some(item) => {
                item.delete();
                Ok(())
            }
            None => clear(ACCOUNT),
        }
    }
}

/// Records are `NAME <byte length>\n<value>\n`. Names are already restricted to
/// `[A-Za-z_][A-Za-z0-9_]*`, so the header cannot be ambiguous, and the length
/// prefix means a value may hold anything at all, newlines included.
fn encode(secrets: &Secrets) -> String {
    let mut out = String::new();
    for (name, value) in secrets {
        out.push_str(name);
        out.push(' ');
        out.push_str(&value.len().to_string());
        out.push('\n');
        out.push_str(value);
        out.push('\n');
    }
    out
}

fn decode(blob: &str) -> Result<Secrets> {
    let mut secrets = Secrets::new();
    let mut rest = blob;

    while !rest.is_empty() {
        let (header, body) = rest
            .split_once('\n')
            .context("keyring contents are truncated: no end to a record header")?;
        let (name, length) = header
            .rsplit_once(' ')
            .with_context(|| format!("malformed record header {header:?} in keyring contents"))?;
        let length: usize = length
            .parse()
            .with_context(|| format!("malformed length in record header {header:?}"))?;

        let value = body
            .get(..length)
            .with_context(|| format!("record for \"{name}\" is truncated"))?;
        if body.as_bytes().get(length) != Some(&b'\n') {
            bail!("record for \"{name}\" is not terminated");
        }

        secrets.insert(name.to_string(), Secret::new(value.to_string()));
        rest = &body[length + 1..];
    }

    Ok(secrets)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secrets(pairs: &[(&str, &str)]) -> Secrets {
        pairs
            .iter()
            .map(|(name, value)| (name.to_string(), Secret::new(value.to_string())))
            .collect()
    }

    fn roundtrip(pairs: &[(&str, &str)]) {
        let decoded = decode(&encode(&secrets(pairs))).expect("decodes");
        assert_eq!(decoded.len(), pairs.len());
        for (name, value) in pairs {
            assert_eq!(&*decoded[*name], *value);
        }
    }

    #[test]
    fn roundtrips_plain_values() {
        roundtrip(&[("GH_TOKEN", "abc123"), ("OTHER", "def")]);
    }

    #[test]
    fn roundtrips_values_with_newlines_and_spaces() {
        roundtrip(&[("KEY", "line one\nline two\n"), ("SPACED", "a b c")]);
    }

    #[test]
    fn roundtrips_empty_value() {
        roundtrip(&[("EMPTY", "")]);
    }

    #[test]
    fn decodes_nothing_from_empty_input() {
        assert!(decode("").expect("decodes").is_empty());
    }

    #[test]
    fn rejects_truncated_value() {
        assert!(decode("KEY 10\nshort\n").is_err());
    }

    #[test]
    fn rejects_unterminated_value() {
        assert!(decode("KEY 3\nabc").is_err());
    }

    #[test]
    fn rejects_malformed_header() {
        assert!(decode("KEY\nabc\n").is_err());
        assert!(decode("KEY xyz\nabc\n").is_err());
    }
}
