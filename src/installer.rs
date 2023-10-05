use async_trait::async_trait;
use bytes::Bytes;
use flate2::read::GzDecoder;
use reqwest::Client;
use semver::Comparator;
use std::fs::File;
use std::io::Write;
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

use crate::{
    cache::{Cache, CACHE_DIRECTORY},
    command_handler::CommandHandler,
    errors::{
        CommandError::{self},
        ParseError::{self, *},
    },
    http::HTTPRequest,
    types::{DependencyMap, PackageLock, VersionData},
    versions::{Versions, LATEST},
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

type DependencyMapMutex = Arc<Mutex<DependencyMap>>;

impl Installer {
    /// Gets the version data taking in the full version rather than resolving it on its own.
    async fn get_version_data(
        client: Client,
        package_name: &String,
        full_version: Option<&String>,
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
        is_latest: bool,
        dependency_map_mux: DependencyMapMutex,
    ) -> bool {
        let mut dependency_map = dependency_map_mux.lock().unwrap();
        let stringified_version = Versions::stringify(name, version);

        let installed_version = dependency_map.get(&stringified_version);

        match installed_version {
            Some(_) => true,
            None => {
                dependency_map.insert(stringified_version, PackageLock::new(is_latest));
                false
            }
        }
    }

    /// Append a version to a specific parent version, this hashmap will be used to generate package lock files.
    fn append_version(
        parent_version_name: String,
        new_version_name: String,
        dependency_map_mux: DependencyMapMutex,
    ) -> Result<(), CommandError> {
        let mut dependency_map = dependency_map_mux.lock().unwrap();
        let parent_version = dependency_map
            .entry(parent_version_name.to_string())
            .or_insert(PackageLock::new(parent_version_name.ends_with(LATEST)));

        parent_version.dependencies.push(new_version_name);

        Ok(())
    }

    fn install_package(
        client: reqwest::Client,
        version_data: VersionData,
        is_latest: bool,
        bytes_sender: Sender<PackageBytes>,
        dependency_map_mux: DependencyMapMutex,
    ) -> Result<(), CommandError> {
        if Self::already_resolved(
            &version_data.name,
            &version_data.version,
            is_latest,
            Arc::clone(&dependency_map_mux),
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
                let full_version_ref = full_version.as_ref();

                let is_cached = Cache::exists(&name, full_version_ref, comparator_ref)
                    .await
                    .unwrap();

                if is_cached {
                    // TODO(conaticus): Handle if in the cache.
                    continue;
                }

                let version_data =
                    Self::get_version_data(client.clone(), &name, full_version_ref, comparator_ref)
                        .await
                        .unwrap();

                // TODO(conaticus): Instead of re-formatting the parent version, this should be only done once
                let stringified_child = Versions::stringify(&name, &version);

                Self::append_version(
                    stringified_parent.clone(),
                    stringified_child,
                    Arc::clone(&dependency_map_mux),
                )
                .unwrap();

                Self::install_package(
                    client.clone(),
                    version_data,
                    Versions::is_latest(full_version),
                    bytes_sender.clone(),
                    Arc::clone(&dependency_map_mux),
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
        let full_version_ref = full_version.as_ref();

        let is_cached =
            Cache::exists(&self.package_name, full_version_ref, semantic_version_ref).await?;

        if is_cached {
            // TODO(conaticus): Handle if in the cache.
            return Ok(());
        }

        let version_data = Self::get_version_data(
            client.clone(),
            &self.package_name,
            full_version_ref,
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

        let dependency_map_mux: DependencyMapMutex = Arc::new(Mutex::new(HashMap::new()));

        Self::install_package(
            client,
            version_data,
            Versions::is_latest(full_version),
            tx,
            Arc::clone(&dependency_map_mux),
        )?;

        // NOTE(conaticus): This is blocking however it's not going to have a huge performance impact on tokio
        while load_task_count() != 0 {}

        let dependency_map = dependency_map_mux.lock().unwrap();
        for (package_name, package_lock) in dependency_map.iter() {
            let mut package_lock_file = File::create(format!(
                "{}/{}/package/click-lock.json",
                *CACHE_DIRECTORY, package_name
            ))
            .map_err(CommandError::FailedToCreateFile)?;

            let package_lock_string = serde_json::to_string(package_lock)
                .map_err(CommandError::FailedToSerializePackageLock)?;

            package_lock_file
                .write_all(package_lock_string.as_bytes())
                .map_err(CommandError::FailedToWriteFile)?;
        }

        println!("elapsed: {}ms", start.elapsed().as_millis());

        Ok(())
    }
}
