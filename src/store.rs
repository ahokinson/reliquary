use anyhow::{Context, Result, bail};
use keyring::Entry;

use crate::secret::Secret;

const SERVICE: &str = "reliquary";

fn entry(name: &str) -> Result<Entry> {
    Entry::new(SERVICE, name).with_context(|| format!("opening keyring entry for \"{name}\""))
}

/// `Ok(None)` means not configured yet, not an error.
pub fn get(name: &str) -> Result<Option<Secret>> {
    match entry(name)?.get_password() {
        Ok(value) => Ok(Some(Secret::new(value))),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(keyring::Error::Ambiguous(_)) => bail!(
            "multiple keyring entries match \"{name}\" — resolve the duplicate via your keyring GUI"
        ),
        Err(e) => Err(e).with_context(|| format!("reading \"{name}\" from the keyring")),
    }
}

pub fn set(name: &str, value: &str) -> Result<()> {
    entry(name)?
        .set_password(value)
        .with_context(|| format!("writing \"{name}\" to the keyring"))
}

/// Treats "already absent" as success.
pub fn delete(name: &str) -> Result<()> {
    match entry(name)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e).with_context(|| format!("deleting \"{name}\" from the keyring")),
    }
}
