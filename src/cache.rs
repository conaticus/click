use std::{
    collections::HashMap,
    fs::{self as fs_sync, File},
    io::{Read, Seek, SeekFrom},
    str::FromStr,
};

use lazy_static::lazy_static;
use semver::{Comparator, Version};
use tokio::fs;

use crate::{
    errors::CommandError,
    versions::{Versions, EMPTY_VERSION, LATEST},
};

lazy_static! {
    pub static ref CACHE_DIRECTORY: String = format!(
        "{}/node-cache",
        dirs::cache_dir()
            .expect("Failed to find cache directory")
            .to_str()
            .expect("Failed to convert cache directory to string")
    );
    pub static ref CACHED_VERSIONS: HashMap<String, bool> = Cache::get_cached_versions();
}

pub struct Cache;
impl Cache {
    /// Returns a hashmap, each key is formatted as package@version and the value is whether the package is the latest version or not
    pub fn get_cached_versions() -> HashMap<String, bool> {
        let dir_contents =
            fs_sync::read_dir(CACHE_DIRECTORY.to_string()).expect("Failed to read cache directory");

        let mut cached_versions = HashMap::new();

        for entry in dir_contents {
            let entry = entry.expect("Failed to get directory entry");
            let filename = entry.file_name().to_string_lossy().to_string();

            let mut lock_file = File::open(format!(
                "{}/{}/package/click-lock.json",
                *CACHE_DIRECTORY, filename
            ))
            .expect("Failed to read package lock file");

            let start_byte = 12;
            let end_byte = 15;

            let bytes_length = end_byte - start_byte + 1;
            let mut buf = vec![0; bytes_length];

            lock_file.seek(SeekFrom::Start(start_byte as u64)).unwrap();
            lock_file.read_exact(&mut buf).unwrap();

            let is_latest_str = String::from_utf8(buf).unwrap();
            let is_latest = is_latest_str == "true";

            cached_versions.insert(filename, is_latest);
        }

        return cached_versions;
    }

    /// Checks if a package with a valid version matching with `semantic_version` is already in the cache
    /// and returns `true` if so, `false` if otherwise.
    pub async fn exists(
        package_name: &String,
        version: Option<&String>,
        semantic_version: Option<&Comparator>,
    ) -> Result<bool, CommandError> {
        if let Some(version) = version {
            if version == LATEST {
                return Ok(Self::latest_is_cached(package_name));
            }

            let stringified_version = Versions::stringify(&package_name, &version);
            return Ok(
                fs::metadata(format!("{}/{}", *CACHE_DIRECTORY, stringified_version))
                    .await
                    .is_ok(),
            );
        }

        let mut cache_entries = fs::read_dir(CACHE_DIRECTORY.to_string())
            .await
            .map_err(CommandError::NoCacheDirectory)?;

        let semantic_version = semantic_version.unwrap();

        while let Some(cache_entry) = cache_entries
            .next_entry()
            .await
            .map_err(CommandError::FailedDirectoryEntry)
            .unwrap()
        {
            let filename = cache_entry.file_name().to_string_lossy().to_string();
            if !filename.starts_with(package_name) {
                continue;
            }

            let (_, entry_version) = Versions::parse_raw_package_details(filename);

            let version = &Version::from_str(entry_version.as_str()).unwrap_or(EMPTY_VERSION);
            if semantic_version.matches(version) {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Checks if the latest version exists in the cache.
    /// This is checked by reading if the package lock has the latest property as true.
    pub fn latest_is_cached(package_name: &String) -> bool {
        match CACHED_VERSIONS.get(package_name) {
            Some(is_latest) => *is_latest,
            None => false,
        }
    }
}
