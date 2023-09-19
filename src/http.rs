use crate::{
    errors::CommandError::{self, *},
    types::{PackageData, VersionData},
};

pub const REGISTRY_URL: &str = "https://registry.npmjs.org";

pub struct HTTPRequest;
impl HTTPRequest {
    /// Make a request to the NPM registry.
    /// This includes the recommended header to shorten the response size.
    async fn registry(client: reqwest::Client, route: String) -> Result<String, CommandError> {
        client
            .get(format!("{REGISTRY_URL}{route}"))
            .header(
                "Accept",
                "application/vnd.npm.install-v1+json; q=1.0, application/json; q=0.8, */*",
            )
            .send()
            .await
            .or_else(|err| Err(HTTPFailed(err)))?
            .text()
            .await
            .or_else(|err| Err(FailedResponseText(err)))
    }

    /// This makes a request for a specific version of a package.
    /// This method should always be preferred where possible as its response size is significantly smaller than full package data.
    pub async fn version_data(
        client: reqwest::Client,
        package_name: &String,
        version: &String,
    ) -> Result<VersionData, CommandError> {
        let response_raw = Self::registry(client, format!("/{package_name}/{version}")).await?;
        serde_json::from_str::<VersionData>(&response_raw).or_else(|err| Err(ParsingFailed(err)))
    }

    /// This makes a request for all data for a package including all its versions.
    /// This method should be avoided where possible as its response size is much larger than just requesting version data.
    pub async fn package_data(
        client: reqwest::Client,
        package_name: &String,
    ) -> Result<PackageData, CommandError> {
        let response_raw = Self::registry(client, format!("/{package_name}")).await?;
        serde_json::from_str::<PackageData>(&response_raw).or_else(|err| Err(ParsingFailed(err)))
    }
}
