use semver::VersionReq;
use std::env::Args;

use crate::{
    command_parser::CommandHandler,
    errors::{
        CommandError,
        ParseError::{self, InvalidVersionNotation, MissingArgument},
    },
};

#[derive(Default)]
pub struct Installer {
    package_name: String,
    package_version: Option<VersionReq>, // If None then assume latest version.
}

impl Installer {
    fn parse_package_details(
        package_details: String,
    ) -> Result<(String, Option<VersionReq>), ParseError> {
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
            Ok(version) => Ok((name, Some(version))),
            Err(err) => Err(InvalidVersionNotation(err)),
        }
    }
}

impl CommandHandler for Installer {
    fn parse(&mut self, args: &mut Args) -> Result<(), ParseError> {
        let package_details = match args.next() {
            Some(package_details) => package_details,
            None => return Err(MissingArgument("package name".to_string())),
        };

        let (package_name, package_version) = Self::parse_package_details(package_details)?;
        self.package_name = package_name;
        self.package_version = package_version;

        Ok(())
    }

    fn execute(&self) -> Result<(), CommandError> {
        println!("Installing '{}'", self.package_name);
        Ok(())
    }
}
