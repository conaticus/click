use std::str::FromStr;

use lazy_static::lazy_static;
use semver::{Comparator, Version};
use tokio::fs;

use crate::{
    errors::CommandError,
    versions::{Versions, EMPTY_VERSION},
};

lazy_static! {
    pub static ref CACHE_DIRECTORY: String = format!(
        "{}/node-cache",
        dirs::cache_dir()
            .expect("Failed to find cache directory")
            .to_str()
            .expect("Failed to convert cache directory to string")
    );
}

pub struct Cache;
impl Cache {
    /// Checks if a package with a valid version matching with `semantic_version` is already in the cache
    /// and returns `true` if so, `false` if otherwise.
    pub async fn exists(
        package_name: &String,
        version: Option<&String>,
        semantic_version: Option<&Comparator>,
    ) -> Result<bool, CommandError> {
        if let Some(version) = version {
            if version == "latest" {
                return Self::latest_is_cached(package_name).await;
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
    pub async fn latest_is_cached(package_name: &String) -> Result<bool, CommandError> {
        todo!()
    }
}
