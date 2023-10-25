use crate::errors::{CommandError, ParseError};
use async_trait::async_trait;
use std::env::Args;
use std::process::Command;
use std::io;

use super::command_handler::CommandHandler;

#[derive(Default)]
pub struct RunFileHandler {
    file_name: String,
}

#[async_trait]
impl CommandHandler for RunFileHandler {
    fn parse(&mut self, args: &mut Args) -> Result<(), ParseError> {
        let parsed_args = args
            .next()
            .ok_or(ParseError::MissingArgument(String::from("file name")))?;

        self.file_name = parsed_args;

        Ok(())
    }
    async fn execute(&self) -> Result<(), CommandError> {
        let cmd = Command::new("node")
            .args(["--preserve-symlinks", &self.file_name])
            .status()
            .map_err(CommandError::ComandFailedError)?;

        if !(cmd.success()) {
            let error_message = "Something went wrong";

            let error = io::Error::new(io::ErrorKind::Other, error_message);
            return Err(CommandError::ComandFailedError(error));
        }

        Ok(())
    }
}
