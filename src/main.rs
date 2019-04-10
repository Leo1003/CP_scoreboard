#![allow(non_snake_case)]

extern crate chrono;
extern crate config;
extern crate ncurses;
#[macro_use]
extern crate prettytable;
extern crate reqwest;

mod error;
mod meta;

use self::meta::Metadata;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let mut meta = Metadata::load()?;
    if meta.get_token().is_empty() {
        return Err("User token not set!".into());
    }

    Ok(())
}
