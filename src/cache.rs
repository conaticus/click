use std::{
    collections::HashMap,
    fs::{self as fs_sync, File},
    io::{ErrorKind, Read, Seek, SeekFrom},
    path::Path,
    str::FromStr,
};

use lazy_static::lazy_static;
use semver::{Comparator, Version};
use tokio::fs;

use crate::{
    errors::CommandError,
    types::PackageLock,
    versions::{Versions, EMPTY_VERSION, LATEST},
};

pub struct CachedVersion {
    pub version: String,
    pub is_latest: bool,
}

pub type CachedVersions = HashMap<String, CachedVersion>;

lazy_static! {
    pub static ref CACHE_DIRECTORY: String = format!(
        "{}/node-cache",
        dirs::cache_dir()
            .expect("Failed to find cache directory")
            .to_str()
            .expect("Failed to convert cache directory to string")
    );
    pub static ref CACHED_VERSIONS: CachedVersions = Cache::get_cached_versions();
}

pub struct Cache;
impl Cache {
    /// Returns a hashmap, each key is formatted as package@version
    /// and the value is a boolean of whether the package is the latest version or not.
    pub fn get_cached_versions() -> CachedVersions {
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

            // This is not an ideal method but it beats parsing the JSON of every installed package
            let start_byte = 12;
            let end_byte = 15;

            let bytes_length = end_byte - start_byte + 1;
            let mut buf = vec![0; bytes_length];

            lock_file.seek(SeekFrom::Start(start_byte as u64)).unwrap();
            lock_file.read_exact(&mut buf).unwrap();

            let is_latest_str = String::from_utf8(buf).unwrap();
            let is_latest = is_latest_str == "true";

            let (name, version) = Versions::parse_raw_package_details(filename);
            cached_versions.insert(name, CachedVersion { version, is_latest });
        }

        cached_versions
    }

    /// Checks if a package with a valid version matching with `semantic_version` is already in the cache
    /// and returns `true` if so, `false` if otherwise, as well as the resolved version if it exists
    pub async fn exists(
        package_name: &String,
        version: Option<&String>,
        semantic_version: Option<&Comparator>,
    ) -> Result<(bool, Option<String>), CommandError> {
        if let Some(version) = version {
            if version == LATEST {
                let latest_version = Self::get_latest_version_in_cache(package_name);
                return Ok((latest_version.is_some(), latest_version));
            }

            return Ok((
                Self::is_in_cache(package_name, version),
                Some(version.to_string()),
            ));
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
                return Ok((true, Some(entry_version)));
            }
        }

        Ok((false, None))
    }

    pub fn is_in_cache(package: &String, version: &String) -> bool {
        let cached_version = CACHED_VERSIONS.get(package);
        match cached_version {
            Some(ver) if &ver.version == version => true,
            _ => false,
        }
    }

    /// Checks if the latest version exists in the cache.
    /// This is checked by reading if the package lock has the latest property as true.
    pub fn get_latest_version_in_cache(package_name: &String) -> Option<String> {
        let cached_version = CACHED_VERSIONS.get(package_name);
        match cached_version {
            Some(ver) if ver.is_latest => Some(ver.version.to_string()),
            _ => None,
        }
    }

    /// Package string is formated as package@version
    pub fn load_cached_version(package: String) {
        let lockfile_raw = fs_sync::read_to_string(format!(
            "{}/{}/package/click-lock.json",
            *CACHE_DIRECTORY, package
        ))
        .expect("Failed to read package lockfile");

        let lockfile = serde_json::from_str::<PackageLock>(lockfile_raw.as_str()).unwrap();
        let mut dependencies = lockfile.dependencies;
        dependencies.push(package);

        for dependency in dependencies {
            let (package_name, _) = Versions::parse_raw_package_details(dependency.to_string());

            let result = symlink::symlink_dir(
                format!("{}/{}/package", *CACHE_DIRECTORY, dependency),
                format!("./node_modules/{}", package_name),
            );

            match result {
                Ok(_) => continue,
                Err(err) if err.kind() == ErrorKind::AlreadyExists => continue,
                Err(err) => panic!("{}", err),
            }
        }
    }
}
