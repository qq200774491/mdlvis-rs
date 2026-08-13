#![allow(dead_code, unused_imports)]

mod reader;
mod syntax;
mod writer;

pub use reader::{load, load_path, parse_str};
pub use writer::{save, save_path, to_string};

#[cfg(test)]
mod tests;
