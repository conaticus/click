use async_trait::async_trait;
use semver::{BuildMetadata, Comparator, Prerelease, Version, VersionReq};
use std::{collections::HashMap, env::Args, str::FromStr};

use crate::{
    command_parser::CommandHandler,
    errors::{
        CommandError,
        CommandError::HTTPFailed,
        CommandError::ParsingFailed,
        ParseError::{self, InvalidVersionNotation, MissingArgument},
    },
    types::{PackageData, VersionData},
};

pub const REGISTRY_URL: &str = "https://registry.npmjs.org";
const EMPTY_VERSION: Version = Version {
    major: 0,
    minor: 0,
    patch: 0,
    pre: Prerelease::EMPTY,
    build: BuildMetadata::EMPTY,
};

#[derive(Default)]
pub struct Installer {
    package_name: String,
    package_version: Option<Comparator>, // If None then assume latest version.
}

impl Installer {
    fn parse_package_details(
        package_details: String,
    ) -> Result<(String, Option<Comparator>), ParseError> {
        let mut split = package_details.split("@");

        let name = split
            .next()
            .expect("Provided package name is empty")
            .to_string();

        let version_raw = match split.next() {
            Some(version_raw) if version_raw == "latest" => return Ok((name, None)),
            Some(version_raw) => version_raw,
            None => return Ok((name, None)),
        };

        let version_result = VersionReq::parse(version_raw);
        match version_result {
            Ok(version) => {
                let comparator = version
                    .comparators
                    .get(0)
                    .expect("Missing version comparator")
                    .clone(); // Annoyingly we have to clone because we can't move out of the array
                Ok((name, Some(comparator)))
            }
            Err(err) => Err(InvalidVersionNotation(err)),
        }
    }

    fn get_best_compatible_version(
        &self,
        available_versions: HashMap<String, VersionData>,
    ) -> String {
        let mut versions = available_versions.iter().collect::<Vec<_>>();
        // Serde scambles the order of the hashmap so we need to reorder it.
        versions.sort_by(|a, b| a.0.cmp(b.0));

        let package_version = match &self.package_version {
            Some(package_version) => package_version,
            None => {
                return versions
                    .last()
                    .expect("No versions available for this package")
                    .0
                    .clone()
            }
        };

        if package_version.minor.is_some() && package_version.patch.is_some() {
            return format!(
                "{}.{}.{}",
                package_version.major,
                package_version.minor.unwrap(),
                package_version.patch.unwrap()
            );
        }

        // Do in reverse order so we find the latest compatible version.
        for (version_str, _) in versions.iter().rev() {
            let version = Version::from_str(version_str).unwrap_or(EMPTY_VERSION);

            if package_version.matches(&version) {
                return (*(*version_str)).clone(); // I hate double references
            }
        }

        panic!("Could not find valid version to download")
    }
}

#[async_trait]
impl CommandHandler for Installer {
    fn parse(&mut self, args: &mut Args) -> Result<(), ParseError> {
        let package_details = match args.next() {
            Some(package_details) => package_details,
            None => return Err(MissingArgument(String::from("package name"))),
        };

        let (package_name, package_version) = Self::parse_package_details(package_details)?;
        self.package_name = package_name;
        self.package_version = package_version;

        Ok(())
    }

    async fn execute(&self) -> Result<(), CommandError> {
        // TODO(conaticus): Check if the package is already in the cache and is valid, if not install another version into the cache
        // In future we could automatically find a version that is valid for both limits to save storage, but that's not neccessary right now
        println!("Installing '{}'..", self.package_name);

        let package_response = reqwest::get(format!("{REGISTRY_URL}/{}", self.package_name)).await;
        let package_data_raw = match package_response {
            Ok(package_data) => package_data
                .text()
                .await
                .expect("Failed to convert HTTP data to text"),
            Err(err) => return Err(HTTPFailed(err)),
        };

        let data_result = serde_json::from_str::<PackageData>(&package_data_raw);
        std::mem::drop(package_data_raw); // We are about to do a bunch of I/O so there is no point keeping this large value in memory

        let package_data = match data_result {
            Ok(data) => data,
            Err(err) => return Err(ParsingFailed(err)),
        };

        let best_version = self.get_best_compatible_version(package_data.versions);
        println!("install the latest possible version: {}", best_version);

        Ok(())
    }
}
