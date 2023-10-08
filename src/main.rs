mod cache;
mod commands;
mod errors;
mod http;
mod installer;
mod types;
mod util;
mod versions;

use std::env;

use commands::command_handler;

#[tokio::main]
async fn main() {
    let parse_result = command_handler::handle_args(env::args()).await;
    if let Err(err) = parse_result {
        println!("Failed to parse command: {err}");
    }
}
