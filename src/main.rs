mod command_handler;
mod errors;
mod http;
mod installer;
mod types;
mod versions;

use std::env;

#[tokio::main]
async fn main() {
    let parse_result = command_handler::handle_args(env::args()).await;
    match parse_result {
        Err(error) => println!("Failed to parse command: {error}"),
        Ok(_) => (),
    }
}
