use std::collections::HashMap;

use serde::Deserialize;

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
