use std::ops::Deref;

use zeroize::Zeroize;

/// An in-memory secret: mlocked against swap for its lifetime, wiped on drop.
pub struct Secret(String);

impl Secret {
    pub fn new(value: String) -> Self {
        lock(&value);
        Secret(value)
    }
}

impl Deref for Secret {
    type Target = str;

    fn deref(&self) -> &str {
        &self.0
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        self.0.zeroize();
        unlock(&self.0);
    }
}

fn lock(value: &str) {
    unsafe {
        libc::mlock(value.as_ptr().cast(), value.len());
    }
}

fn unlock(value: &str) {
    unsafe {
        libc::munlock(value.as_ptr().cast(), value.len());
    }
}
