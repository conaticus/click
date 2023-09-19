use std::io::Error;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("command '{0}' not found")]
    CommandNotFound(String),
    #[error("missing argument: '{0}'")]
    MissingArgument(String),
    #[error("invalid version notation ({0})")]
    InvalidVersionNotation(semver::Error),
}

#[derive(Error, Debug)]
pub enum CommandError {
    #[error("failed to execute http request ({0})")]
    HTTPFailed(reqwest::Error),
    #[error("failed to parse http data to struct via json ({0})")]
    ParsingFailed(serde_json::Error),
    #[error("failed to get http response text ({0})")]
    FailedResponseText(reqwest::Error),
    #[error("failed to get http response bytes ({0})")]
    FailedResponseBytes(reqwest::Error),
    #[error("the package version you provided was invalid or does not exist")]
    InvalidVersion,
    #[error("failed to extract tar file ({0})")]
    ExtractionFailed(Error),
    // NOTE(conaticus): I don't like repeating this in the command errors, might find a better work around later
    #[error("invalid version notation ({0})")]
    InvalidVersionNotation(semver::Error),
}
