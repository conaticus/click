mod command_parser;
mod errors;
mod installer;

use std::env;

fn main() {
    let parse_result = command_parser::handle_args(env::args());
    match parse_result {
        Err(error) => println!("Failed to parse command: {error}"),
        Ok(_) => (),
    }
}
