use async_trait::async_trait;
use std::env::Args;

use crate::errors::{
    CommandError,
    ParseError::{self, CommandNotFound},
};

use super::install::InstallHandler;
use super::exec::RunFileHandler;

#[async_trait]
pub trait CommandHandler {
    fn parse(&mut self, args: &mut Args) -> Result<(), ParseError>;
    async fn execute(&self) -> Result<(), CommandError>;
}

pub async fn handle_args(mut args: Args) -> Result<(), ParseError> {
    args.next(); // Remove initial binary argument

    let command = match args.next() {
        Some(command) => command,
        None => {
            println!("Use: click <command> [options]\n  click install <package_name> [semver]\n  click exec <file name>");
            return Ok(());
        }
    };

    let mut command_handler: Box<dyn CommandHandler> = match command.to_lowercase().as_str() {
        "install" => Box::<InstallHandler>::default(),
        "exec" => Box::<RunFileHandler>::default(),
        _ => return Err(CommandNotFound(command.to_string())),
    };

    command_handler.parse(&mut args)?;
    let command_result = command_handler.execute().await;

    if let Err(e) = command_result {
        println!("Command error: {e}");
    }

    Ok(())
}
