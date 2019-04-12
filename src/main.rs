#![allow(non_snake_case)]

extern crate chrono;
#[macro_use]
extern crate custom_error;
extern crate cursive;
#[macro_use]
extern crate prettytable;
extern crate bincode;
extern crate reqwest;
extern crate serde;
extern crate term;
extern crate toml;

mod error;
mod meta;
mod scoreboard;

use self::meta::Metadata;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let meta = Metadata::load()?;
    if meta.get_token().is_empty() {
        return Err("User token not set!".into());
    }

    let mut board = scoreboard::Scoreboard::new();
    for &pid in meta.problems() {
        board.add_problem(pid);
    }
    scoreboard::sync(&mut board, meta.get_token())?;
    // if let Some(mut tty) = term::stdout() {
    //     board.gen_table().print_term(tty.as_mut());
    // } else {

    // }
    board.gen_table().printstd();

    Ok(())
}
