use async_trait::async_trait;
use std::env::Args;

use crate::errors::{
    CommandError,
    ParseError::{self, CommandNotFound},
};

use super::install::InstallHandler;

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
            // TODO(conaticus): Implement help menu
            println!("No help menu implemented yet.");
            return Ok(());
        }
    };

    let mut command_handler: Box<dyn CommandHandler> = match command.to_lowercase().as_str() {
        "install" => Box::<InstallHandler>::default(),
        _ => return Err(CommandNotFound(command.to_string())),
    };

    command_handler.parse(&mut args)?;
    let command_result = command_handler.execute().await;

    if let Err(e) = command_result {
        println!("Command error: {e}");
    }

    Ok(())
}
