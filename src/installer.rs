use async_trait::async_trait;
use bytes::Bytes;
use flate2::read::GzDecoder;
use reqwest::Client;
use semver::Comparator;
use std::{
    collections::HashMap,
    env::Args,
    sync::{
        atomic::{self, AtomicUsize},
        mpsc::{channel, Sender},
        Arc, Mutex,
    },
    time::Instant,
};
use tar::Archive;
use tokio::fs;

use crate::{
    cache::{Cache, CACHE_DIRECTORY},
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

fn increment_task_count() {
    ACTIVE_TASK_COUNT.fetch_add(1, atomic::Ordering::SeqCst);
}

fn decrement_task_count() {
    ACTIVE_TASK_COUNT.fetch_sub(1, atomic::Ordering::SeqCst);
}

fn load_task_count() -> usize {
    ACTIVE_TASK_COUNT.load(atomic::Ordering::SeqCst)
}

type PackageBytes = (String, Bytes); // Package destination, package bytes

type InstalledVersionsMutex = Arc<Mutex<HashMap<String, Vec<String>>>>;

impl Installer {
    /// Gets the version data taking in the full version rather than resolving it on its own.
    async fn get_version_data(
        client: Client,
        package_name: &String,
        full_version: Option<String>,
        semantic_version: Option<&Comparator>,
    ) -> Result<VersionData, CommandError> {
        if let Some(version) = full_version {
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

    // NOTE(conaticus): To save storage space, it might be an idea to check if the semantic version matches,
    // rather than installing an whole new version, however this is an uncommon case due to how we handle version resolution so it's not a big deal.
    /// Returns true if a given dependency's version has been/will be installed to avoid unneccesary duplicate installs
    /// If the dependency is not in the hashmap, it will be added to the hashmap for further checks.
    fn already_resolved(
        name: &String,
        version: &String,
        installed_dependencies_mux: InstalledVersionsMutex,
    ) -> bool {
        let mut installed_dependencies = installed_dependencies_mux.lock().unwrap();
        let stringified_version = Versions::stringify(name, version);

        let installed_version = installed_dependencies.get(&stringified_version);

        match installed_version {
            Some(_) => true,
            None => {
                installed_dependencies.insert(stringified_version, Vec::new());
                false
            }
        }
    }

    /// Append a version to a specific parent version, this hashmap will be used to generate package lock files.
    fn append_version(
        parent_version_name: String,
        new_version_name: String,
        installed_dependencies_mux: InstalledVersionsMutex,
    ) -> Result<(), CommandError> {
        let mut installed_dependencies = installed_dependencies_mux.lock().unwrap();
        let parent_version = installed_dependencies
            .entry(parent_version_name)
            .or_insert(Vec::new());

        parent_version.push(new_version_name);

        Ok(())
    }

    fn install_package(
        client: reqwest::Client,
        version_data: VersionData,
        bytes_sender: Sender<PackageBytes>,
        installed_dependencies_mux: InstalledVersionsMutex,
    ) -> Result<(), CommandError> {
        if Self::already_resolved(
            &version_data.name,
            &version_data.version,
            Arc::clone(&installed_dependencies_mux),
        ) {
            return Ok(());
        }

        let package_dest = format!(
            "{}/{}",
            *CACHE_DIRECTORY,
            Versions::stringify(&version_data.name, &version_data.version)
        );

        let tarball_url = version_data.dist.tarball.clone();

        tokio::spawn(async move {
            increment_task_count();

            let stringified_parent = Versions::stringify(&version_data.name, &version_data.version);

            let bytes = HTTPRequest::get_bytes(client.clone(), tarball_url)
                .await
                .unwrap();

            // TODO(conaticus): Do this outside of tokio tasks as it's blocking the threads from working at full potential
            bytes_sender.send((package_dest, bytes)).unwrap();

            let dependencies = version_data.dependencies.unwrap_or(HashMap::new());

            for (name, version) in dependencies {
                let comparator = Versions::parse_semantic_version(&version)
                    .expect("Failed to parse semantic version"); // TODO(conaticus): Change this to return a result

                let comparator_ref = Some(&comparator);

                let full_version = Versions::resolve_full_version(comparator_ref);
                let is_cached = Cache::exists(&name, full_version.as_ref(), comparator_ref)
                    .await
                    .unwrap();

                if is_cached {
                    // TODO(conaticus): Handle if in the cache.
                    continue;
                }

                let version_data =
                    Self::get_version_data(client.clone(), &name, full_version, comparator_ref)
                        .await
                        .unwrap();

                // TODO(conaticus): Instead of re-formatting the parent version, this should be only done once
                let stringified_child = Versions::stringify(&name, &version);

                Self::append_version(
                    stringified_parent.clone(),
                    stringified_child,
                    Arc::clone(&installed_dependencies_mux),
                )
                .unwrap();

                Self::install_package(
                    client.clone(),
                    version_data,
                    bytes_sender.clone(),
                    Arc::clone(&installed_dependencies_mux),
                )
                .unwrap();
            }

            decrement_task_count();
        });

        Ok(())
    }

    // NOTE(conaticus): Later this will likely need to be moved so it can be reused
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

        let (package_name, semantic_version) =
            Versions::parse_semantic_package_details(package_details)?;
        self.package_name = package_name;
        self.semantic_version = semantic_version;

        Ok(())
    }

    async fn execute(&self) -> Result<(), CommandError> {
        // In future we could automatically find a version that is valid for both limits to save storage, but that's not neccessary right now
        println!("Installing '{}'..", self.package_name);

        let client = reqwest::Client::new();
        let start = Instant::now();

        let semantic_version_ref = self.semantic_version.as_ref();

        let full_version = Versions::resolve_full_version(semantic_version_ref);
        let is_cached = Cache::exists(
            &self.package_name,
            full_version.as_ref(),
            semantic_version_ref,
        )
        .await?;

        if is_cached {
            // TODO(conaticus): Handle if in the cache.
            return Ok(());
        }

        let version_data = Self::get_version_data(
            client.clone(),
            &self.package_name,
            full_version,
            self.semantic_version.as_ref(),
        )
        .await?;

        let (tx, rx) = channel::<PackageBytes>();

        tokio::task::spawn_blocking(move || {
            increment_task_count();

            while let Ok((package_dest, bytes)) = rx.recv() {
                Installer::extract_tarball(bytes, package_dest).unwrap();
            }

            decrement_task_count();
        });

        let installed_dependencies_mux: InstalledVersionsMutex =
            Arc::new(Mutex::new(HashMap::new()));
        Self::install_package(
            client,
            version_data,
            tx,
            Arc::clone(&installed_dependencies_mux),
        )?;

        // NOTE(conaticus): This is blocking however it's not going to have a huge performance impact on tokio
        while load_task_count() != 0 {}

        let installed_dependencies = installed_dependencies_mux.lock().unwrap();

        println!("elapsed: {}ms", start.elapsed().as_millis());

        Ok(())
    }
}
