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
    let blob = match entry(ACCOUNT)?.get_password() {
        Ok(blob) => Secret::new(blob),
        Err(keyring::Error::NoEntry) => return Ok(Secrets::new()),
        Err(keyring::Error::Ambiguous(_)) => bail!(
            "multiple keyring entries match \"{SERVICE}\" — resolve the duplicate via your keyring GUI"
        ),
        Err(e) => return Err(e).context("reading the keyring"),
    };
    decode(&blob)
}

pub fn save(secrets: &Secrets) -> Result<()> {
    if secrets.is_empty() {
        return clear();
    }
    let blob = Secret::new(encode(secrets));
    entry(ACCOUNT)?
        .set_password(&blob)
        .context("writing to the keyring")
}

pub fn set(name: &str, value: &str) -> Result<()> {
    let mut secrets = load()?;
    secrets.insert(name.to_string(), Secret::new(value.to_string()));
    save(&secrets)
}

/// Treats "already absent" as success.
pub fn delete(name: &str) -> Result<()> {
    let mut secrets = load()?;
    if secrets.remove(name).is_none() {
        return Ok(());
    }
    save(&secrets)
}

fn clear() -> Result<()> {
    match entry(ACCOUNT)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e).context("deleting the keyring entry"),
    }
}

/// Reads a secret stored under the old one-entry-per-name layout.
pub fn load_separate(name: &str) -> Result<Option<Secret>> {
    match entry(name)?.get_password() {
        Ok(value) => Ok(Some(Secret::new(value))),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e).with_context(|| format!("reading \"{name}\" from the keyring")),
    }
}

pub fn delete_separate(name: &str) -> Result<()> {
    match entry(name)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e).with_context(|| format!("deleting \"{name}\" from the keyring")),
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
