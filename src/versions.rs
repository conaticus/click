use std::{collections::HashMap, str::FromStr};

use semver::{BuildMetadata, Comparator, Op, Prerelease, Version, VersionReq};

use crate::{
    errors::{CommandError, ParseError},
    types::VersionData,
};

pub const EMPTY_VERSION: Version = Version {
    major: 0,
    minor: 0,
    patch: 0,
    pre: Prerelease::EMPTY,
    build: BuildMetadata::EMPTY,
};

pub const LATEST: &str = "latest";

type PackageDetails = (String, Option<Comparator>);

pub struct Versions;
impl Versions {
    pub fn parse_raw_package_details(details: String) -> (String, String) {
        let mut split = details.split('@');

        let name = split
            .next()
            .expect("Provided package name is empty")
            .to_string();

        match split.next() {
            Some(version_raw) => (name, version_raw.to_string()),
            None => (name, LATEST.to_string()),
        }
    }

    pub fn parse_semantic_version(raw_version: &str) -> Result<Comparator, ParseError> {
        let mut version =
            VersionReq::parse(raw_version).map_err(ParseError::InvalidVersionNotation)?;
        Ok(version.comparators.remove(0))
    }

    pub fn parse_semantic_package_details(details: String) -> Result<PackageDetails, ParseError> {
        let (name, version_raw) = Self::parse_raw_package_details(details);

        if version_raw == LATEST {
            return Ok((name, None));
        }

        let comparator = Self::parse_semantic_version(&version_raw)?;
        Ok((name, Some(comparator)))
    }

    /// If a version comparator has the major, patch and minor available a string version will be returned with the resolved version.
    /// This version string can be used to retrieve a package version from the NPM registry.
    /// If the version is not resolvable without requesting the full package data, None will be returned.
    /// None will also be returned if the version operator is Op::Less (<?.?.?) because we need all versions to get the latest version less than this
    pub fn resolve_full_version(semantic_version: Option<&Comparator>) -> Option<String> {
        let latest = LATEST.to_string();

        let semantic_version = match semantic_version {
            Some(semantic_version) => semantic_version,
            None => return Some(latest),
        };

        let (minor, patch) = match (semantic_version.minor, semantic_version.patch) {
            (Some(minor), Some(patch)) => (minor, patch),
            _ => return None,
        };

        match semantic_version.op {
            Op::Greater | Op::GreaterEq | Op::Wildcard => Some(latest),
            Op::Exact | Op::LessEq | Op::Tilde | Op::Caret => Some(Self::stringify_from_numbers(
                semantic_version.major,
                minor,
                patch,
            )),
            _ => None,
        }
    }

    /// Should only be executed if the version comparator is missing a minor or patch.
    /// This can be checked with resolve_full_version() which will return None if this is the case.
    pub fn resolve_partial_version(
        semantic_version: Option<&Comparator>,
        available_versions: &HashMap<String, VersionData>,
    ) -> Result<String, CommandError> {
        let semantic_version = semantic_version
            .expect("Function should not be called as the version can be resolved to 'latest'");

        let mut versions = available_versions.iter().collect::<Vec<_>>();

        // Serde scambles the order of the hashmap so we need to reorder it to find the latest versions
        Self::sort(&mut versions);

        if semantic_version.op == Op::Less {
            // Annoyingly we can't put `if let` and other comparisons on the same line as it's unstable as of writing
            if let (Some(minor), Some(patch)) = (semantic_version.minor, semantic_version.patch) {
                let version_position = versions
                    .iter()
                    .position(|(ver, _)| {
                        ver == &&Self::stringify_from_numbers(semantic_version.major, minor, patch)
                    })
                    .ok_or(CommandError::InvalidVersion)?;

                return Ok(versions
                    .get(version_position - 1)
                    .expect("Invalid version provided (no smaller versions available)")
                    .0
                    .to_string());
            }
        }

        // Do in reverse order so we find the latest compatible version.
        for (version_str, _) in versions.iter().rev() {
            let version = Version::from_str(version_str.as_str()).unwrap_or(EMPTY_VERSION);

            if semantic_version.matches(&version) {
                return Ok(version_str.to_string());
            }
        }

        Err(CommandError::InvalidVersion)
    }

    pub fn stringify(name: &String, version: &String) -> String {
        format!("{}@{}", name, version)
    }

    /// Takes in a result of Versions::resolve_full_version()
    pub fn is_latest(version_string: Option<&String>) -> bool {
        match version_string {
            Some(version) => version == LATEST,
            None => false,
        }
    }

    // This might not be effective for versions that include a prerelease in the version (experimental, canary etc)
    fn sort(versions_vec: &mut [(&String, &VersionData)]) {
        versions_vec.sort_by(|a, b| a.0.cmp(b.0))
    }

    pub fn stringify_from_numbers(major: u64, minor: u64, patch: u64) -> String {
        format!("{}.{}.{}", major, minor, patch)
    }
}
