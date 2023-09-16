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
pub enum CommandError {}
