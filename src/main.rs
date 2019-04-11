#![allow(non_snake_case)]

extern crate chrono;
extern crate config;
extern crate cookie;
extern crate cursive;
#[macro_use]
extern crate prettytable;
extern crate reqwest;
extern crate term;

mod error;
mod meta;
mod scoreboard;

use self::meta::Metadata;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let mut meta = Metadata::load()?;
    if meta.get_token().is_empty() {
        return Err("User token not set!".into());
    }

    let mut board = scoreboard::Scoreboard::new();
    board.add_problem(819);
    board.add_problem(820);
    board.add_problem(822);
    board.add_problem(823);
    board.add_problem(825);
    board.add_problem(826);
    board.add_problem(829);
    board.add_problem(830);
    board.add_problem(843);
    board.add_problem(844);
    scoreboard::sync(&mut board, meta.get_token())?;
    // if let Some(mut tty) = term::stdout() {
    //     board.gen_table().print_term(tty.as_mut());
    // } else {

    // }
    board.gen_table().printstd();

    Ok(())
}
