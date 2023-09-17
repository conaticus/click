mod command_parser;
mod errors;
mod installer;
mod types;

use std::env;

#[tokio::main]
async fn main() {
    let parse_result = command_parser::handle_args(env::args()).await;
    match parse_result {
        Err(error) => println!("Failed to parse command: {error}"),
        Ok(_) => (),
    }
}
