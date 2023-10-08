use bytes::Bytes;
use semver::Comparator;
use std::fs::{self};
use std::path::Path;
use std::{
    collections::HashMap,
    sync::{mpsc::Sender, Arc, Mutex},
};

use crate::util::TaskAllocator;
use crate::{
    cache::{Cache, CACHE_DIRECTORY},
    errors::CommandError::{self},
    http::HTTPRequest,
    types::{DependencyMap, PackageLock, VersionData},
    versions::{Versions, LATEST},
};

pub type DependencyMapMutex = Arc<Mutex<DependencyMap>>;
pub type PackageBytes = (String, Bytes); // Package destination, package bytes

pub struct PackageInfo {
    pub version_data: VersionData,
    pub is_latest: bool,
    pub stringified: String,
}

#[derive(Clone)]
pub struct InstallContext {
    pub client: reqwest::Client,
    pub bytes_sender: Sender<PackageBytes>,
    pub dependency_map_mux: DependencyMapMutex,
}

pub struct Installer;
impl Installer {
    /// Gets the version data taking in the full version rather than resolving it on its own.
    pub async fn get_version_data(
        client: reqwest::Client,
        package_name: &String,
        full_version: Option<&String>,
        semantic_version: Option<&Comparator>,
    ) -> Result<VersionData, CommandError> {
        if let Some(version) = full_version {
            return HTTPRequest::version_data(client.clone(), package_name, version).await;
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
    fn already_resolved(context: &InstallContext, package_info: &PackageInfo) -> bool {
        let mut dependency_map = context.dependency_map_mux.lock().unwrap();
        let stringified_version = Versions::stringify(
            &package_info.version_data.name,
            &package_info.version_data.version,
        );

        let installed_version = dependency_map.get(&stringified_version);

        match installed_version {
            Some(_) => true,
            None => {
                dependency_map.insert(
                    stringified_version,
                    PackageLock::new(package_info.is_latest),
                );
                false
            }
        }
    }

    /// Append a version to a specific parent version, this hashmap will be used to generate package lock files.
    fn append_version(
        parent_version_name: &String,
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

    pub fn install_package(
        context: InstallContext,
        package_info: PackageInfo,
    ) -> Result<(), CommandError> {
        if Self::already_resolved(&context, &package_info) {
            return Ok(());
        }

        TaskAllocator::add_task(async move {
            let version_data = package_info.version_data;

            let package_bytes =
                HTTPRequest::get_bytes(context.client.clone(), version_data.dist.tarball)
                    .await
                    .unwrap();

            let package_destination = format!("{}/{}", *CACHE_DIRECTORY, package_info.stringified);

            // TODO(conaticus): Do this outside of tokio tasks as it's blocking the threads from working at full potential
            context
                .bytes_sender
                .send((package_destination, package_bytes))
                .unwrap();

            let dependencies = version_data.dependencies.unwrap_or(HashMap::new());
            Self::install_dependencies(&package_info.stringified, context, dependencies).await;
        });

        Ok(())
    }

    async fn install_dependencies(
        parent: &String,
        context: InstallContext,
        dependencies: HashMap<String, String>,
    ) {
        for (name, version) in dependencies {
            let comparator = Versions::parse_semantic_version(&version)
                .expect("Failed to parse semantic version"); // TODO(conaticus): Change this to return a result

            let comparator_ref = Some(&comparator);

            let full_version = Versions::resolve_full_version(comparator_ref);
            let full_version_ref = full_version.as_ref();

            let (is_cached, cached_version) =
                Cache::exists(&name, full_version_ref, comparator_ref)
                    .await
                    .unwrap();

            if is_cached {
                let version = full_version
                    .or(cached_version)
                    .expect("Could not resolve version of cached package");

                let stringified = Versions::stringify(&name, &version);
                Cache::load_cached_version(stringified);

                continue;
            }

            let version_data = Self::get_version_data(
                context.client.clone(),
                &name,
                full_version_ref,
                comparator_ref,
            )
            .await
            .unwrap();

            let stringified = Versions::stringify(&name, &version_data.version);

            Self::append_version(
                parent,
                stringified.to_string(),
                Arc::clone(&context.dependency_map_mux),
            )
            .unwrap();

            let package_info = PackageInfo {
                version_data,
                is_latest: Versions::is_latest(Some(&stringified)),
                stringified,
            };

            Self::install_package(context.clone(), package_info).unwrap();
        }
    }

    /// Creates the node modules folder if it is not present.
    fn create_modules_dir() {
        if Path::new("./node_modules").exists() {
            return;
        }

        fs::create_dir("./node_modules").expect("Failed to create node modules folder");
    }
}
