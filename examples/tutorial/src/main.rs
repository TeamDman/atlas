//! Command-line entry point.
use tutorial::{check::check, emit::emit, parse::parse};

fn main() {
    let text = std::env::args().nth(1).unwrap_or_default();
    match check(parse(&text)) {
        Ok(program) => println!("{}", emit(&program)),
        Err(message) => eprintln!("error: {message}"),
    }
}
