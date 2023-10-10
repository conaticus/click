use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct VersionData {
    pub name: String,
    pub version: String,
    pub dependencies: Option<HashMap<String, String>>,
    pub dist: Dist,
}

#[derive(Debug, Deserialize)]
pub struct Dist {
    pub tarball: String,
}

// This does not include the full package data as we don't need it at the moment.
#[derive(Deserialize)]
pub struct PackageData {
    pub versions: HashMap<String, VersionData>,
}

#[derive(Serialize, Deserialize)]
pub struct PackageLock {
    #[serde(rename = "isLatest")]
    pub is_latest: bool,
    pub dependencies: Vec<String>,
}

impl PackageLock {
    pub fn new(is_latest: bool) -> Self {
        Self {
            is_latest,
            dependencies: Vec::new(),
        }
    }
}

pub type DependencyMap = HashMap<String, PackageLock>;
