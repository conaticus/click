use async_trait::async_trait;
use semver::Comparator;
use std::{env::Args, time::Instant};

use crate::{
    command_handler::CommandHandler,
    errors::{
        CommandError::{self},
        ParseError::{self, *},
    },
    http::HTTPRequest,
    types::VersionData,
    versions::Versions,
};

#[derive(Default)]
pub struct Installer {
    package_name: String,
    semantic_version: Option<Comparator>, // If None then assume latest version.
}

impl Installer {
    async fn install_package(
        client: reqwest::Client,
        version_data: &VersionData,
    ) -> Result<(), CommandError> {
        todo!()
    }
}

#[async_trait]
impl CommandHandler for Installer {
    fn parse(&mut self, args: &mut Args) -> Result<(), ParseError> {
        let package_details = args
            .next()
            .ok_or(MissingArgument(String::from("package name")))?;

        let (package_name, semantic_version) = Versions::parse_package_details(package_details)?;
        self.package_name = package_name;
        self.semantic_version = semantic_version;

        Ok(())
    }

    async fn execute(&self) -> Result<(), CommandError> {
        // TODO(conaticus): Check if a valid version is in the cache and is valid, if not install another version into the cache
        // In future we could automatically find a version that is valid for both limits to save storage, but that's not neccessary right now
        println!("Installing '{}'..", self.package_name);

        let client = reqwest::Client::new();
        let now = Instant::now();

        let package_version = Versions::resolve_full_version(self.semantic_version.as_ref());

        if let Some(version) = package_version {
            let version_data =
                &HTTPRequest::version_data(client.clone(), &self.package_name, &version).await?;
            Self::install_package(client, version_data).await?;
        } else {
            let package_data =
                HTTPRequest::package_data(client.clone(), &self.package_name).await?;
            let package_version = Versions::resolve_partial_version(
                self.semantic_version.as_ref(),
                &package_data.versions,
            )?;

            let version_data = package_data.versions.get(&package_version).expect("");
            Self::install_package(client, version_data).await?;
        }

        let elapsed = now.elapsed();
        println!("Elapsed: {}ms", elapsed.as_millis());

        Ok(())
    }
}
