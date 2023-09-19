use async_trait::async_trait;
use bytes::Bytes;
use flate2::read::GzDecoder;
use reqwest::Client;
use semver::{Comparator, VersionReq};
use std::{collections::HashMap, env::Args, sync::atomic::AtomicUsize, time::Instant};
use tar::Archive;

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

static ACTIVE_TASK_COUNT: AtomicUsize = AtomicUsize::new(0);

impl Installer {
    async fn get_version_data(
        client: Client,
        package_name: &String,
        semantic_version: Option<&Comparator>,
    ) -> Result<VersionData, CommandError> {
        if let Some(version) = Versions::resolve_full_version(semantic_version) {
            return HTTPRequest::version_data(client.clone(), package_name, &version).await;
        }

        let mut package_data = HTTPRequest::package_data(client.clone(), package_name).await?;
        let package_version =
            Versions::resolve_partial_version(semantic_version, &package_data.versions)?;

        Ok(package_data
            .versions
            .remove(&package_version)
            .expect("Failed to find resolved package version in package data"))
    }

    fn install_package(
        client: reqwest::Client,
        version_data: VersionData,
    ) -> Result<(), CommandError> {
        let cache_dir = dirs::cache_dir().expect("Could not find cache directory");
        let package_dest = format!(
            "{}/node-cache/{}@{}",
            cache_dir
                .to_str()
                .expect("Couldn't convert PathBuf to &str"),
            version_data.name,
            version_data.version
        );

        let tarball_url = version_data.dist.tarball.clone();

        ACTIVE_TASK_COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        tokio::spawn(async move {
            let bytes = HTTPRequest::get_bytes(client.clone(), tarball_url)
                .await
                .unwrap();
            Self::extract_tarball(bytes, package_dest).unwrap();

            let dependencies = version_data.dependencies.unwrap_or(HashMap::new());

            for (name, version) in dependencies {
                let semantic_version = VersionReq::parse(version.as_str())
                    .map_err(CommandError::InvalidVersionNotation)
                    .unwrap();
                let comparator = &semantic_version.comparators[0];

                let version_data = Self::get_version_data(client.clone(), &name, Some(comparator))
                    .await
                    .unwrap();
                Self::install_package(client.clone(), version_data).unwrap();
            }

            ACTIVE_TASK_COUNT.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        });

        Ok(())
    }

    // NOTE(conaticus): Later this will likely need to be moved so it can be reused
    /// To be used outside of the tokio HTTP tasks as it would block them
    fn extract_tarball(bytes: Bytes, dest: String) -> Result<(), CommandError> {
        let bytes = &bytes.to_vec()[..];
        let gz = GzDecoder::new(bytes);
        let mut archive = Archive::new(gz);

        // NOTE(conaticus): All tarballs contain a /package directory to the module source, this should be removed later to keep things as clean as possible
        archive
            .unpack(&dest)
            .map_err(CommandError::ExtractionFailed)
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
        let start = Instant::now();

        let version_data = Self::get_version_data(
            client.clone(),
            &self.package_name,
            self.semantic_version.as_ref(),
        )
        .await?;

        Self::install_package(client, version_data)?;
        while ACTIVE_TASK_COUNT.load(std::sync::atomic::Ordering::SeqCst) != 0 {}

        println!("elapsed: {}ms", start.elapsed().as_millis());

        Ok(())
    }
}
