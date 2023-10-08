use std::{
    collections::HashMap,
    env::Args,
    fs::File,
    io::Write,
    sync::{mpsc::channel, Arc, Mutex},
};

use async_trait::async_trait;
use semver::Comparator;

use crate::{
    cache::{Cache, CACHE_DIRECTORY},
    errors::{CommandError, ParseError},
    installer::{DependencyMapMutex, InstallContext, Installer, PackageBytes, PackageInfo},
    util::{self, TaskAllocator},
    versions::Versions,
};

use super::command_handler::CommandHandler;

#[derive(Default)]
pub struct InstallHandler {
    package_name: String,
    semantic_version: Option<Comparator>, // If None then assume latest version.
}

impl InstallHandler {
    fn write_lockfiles(dependency_map_mux: DependencyMapMutex) -> Result<(), CommandError> {
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

        Ok(())
    }
}

#[async_trait]
impl CommandHandler for InstallHandler {
    fn parse(&mut self, args: &mut Args) -> Result<(), ParseError> {
        let package_details = args
            .next()
            .ok_or(ParseError::MissingArgument(String::from("package name")))?;

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
        let semantic_version = self.semantic_version.as_ref();
        let full_version = Versions::resolve_full_version(semantic_version);
        let full_version = full_version.as_ref();

        let (is_cached, cached_version) =
            Cache::exists(&self.package_name, full_version, semantic_version).await?;

        if is_cached {
            let version = full_version
                .or(cached_version.as_ref())
                .expect("Could not resolve version of cached package");

            Cache::load_cached_version(Versions::stringify(&self.package_name, version));

            return Ok(());
        }

        let version_data = Installer::get_version_data(
            client.clone(),
            &self.package_name,
            full_version,
            semantic_version,
        )
        .await?;

        let task_allocator = TaskAllocator::new();
        let (bytes_sender, bytes_receiver) = channel::<PackageBytes>();

        task_allocator.add_blocking(move || {
            while let Ok((package_dest, bytes)) = bytes_receiver.recv() {
                util::extract_tarball(bytes, package_dest).unwrap();
            }
        });

        let dependency_map_mux = Arc::new(Mutex::new(HashMap::new()));

        let install_context = InstallContext {
            client,
            bytes_sender,
            dependency_map_mux: Arc::clone(&dependency_map_mux),
        };

        let stringified = Versions::stringify(&version_data.name, &version_data.version);

        let package_info = PackageInfo {
            version_data,
            is_latest: Versions::is_latest(full_version),
            stringified,
        };

        Installer::install_package(&task_allocator, install_context, package_info)?;

        // Blocks the main thread however it's not going to have a huge performance impact on tokio
        task_allocator.block_until_done();

        Self::write_lockfiles(dependency_map_mux)
    }
}
