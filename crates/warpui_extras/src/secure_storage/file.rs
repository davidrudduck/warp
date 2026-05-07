//! File-based [`SecureStorage`] for build configurations where the OS
//! keychain is unavailable or undesirable (e.g. macOS debug builds that
//! produce a different binary signature on every recompile, causing repeated
//! Keychain password prompts).

use std::path::PathBuf;

use super::Error;

pub struct SecureStorage {
    service_name: String,
    storage_dir: PathBuf,
}

impl SecureStorage {
    pub fn new(service_name: &str, storage_dir: PathBuf) -> Self {
        Self {
            service_name: service_name.to_string(),
            storage_dir,
        }
    }

    fn storage_file(&self, key: &str) -> PathBuf {
        let filename = format!("{}-{key}", self.service_name);
        self.storage_dir.join(filename)
    }
}

impl super::SecureStorage for SecureStorage {
    fn write_value(&self, key: &str, value: &str) -> Result<(), Error> {
        let storage_file = self.storage_file(key);
        if let Some(parent) = storage_file.parent() {
            std::fs::create_dir_all(parent).map_err(|err| Error::Unknown(err.into()))?;
        }
        std::fs::write(storage_file, value.as_bytes()).map_err(|err| Error::Unknown(err.into()))
    }

    fn read_value(&self, key: &str) -> Result<String, Error> {
        let storage_file = self.storage_file(key);
        let bytes = std::fs::read(&storage_file).map_err(|err| match err.kind() {
            std::io::ErrorKind::NotFound => Error::NotFound,
            _ => Error::Unknown(err.into()),
        })?;
        String::from_utf8(bytes).map_err(|err| Error::DecodeError(err.utf8_error()))
    }

    fn remove_value(&self, key: &str) -> Result<(), Error> {
        let storage_file = self.storage_file(key);
        std::fs::remove_file(storage_file).map_err(|err| match err.kind() {
            std::io::ErrorKind::NotFound => Error::NotFound,
            _ => Error::Unknown(err.into()),
        })
    }
}
