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
mod fake_term;
mod meta;
mod scoreboard;

use term::Terminal as _;
use self::meta::Metadata;
use std::error::Error;
use cursive::Cursive;
use cursive::theme::*;
use cursive::views::{ScrollView, TextView};

fn main() -> Result<(), Box<dyn Error>> {
    let mut palette = Palette::default();
    palette[PaletteColor::Background] = Color::Dark(BaseColor::Black);
    palette[PaletteColor::Primary] = Color::Dark(BaseColor::White);
    palette[PaletteColor::View] = Color::Dark(BaseColor::Black);
    palette[PaletteColor::Shadow] = Color::Light(BaseColor::Black);

    let mut theme = Theme::default();
    theme.shadow = false;
    theme.palette = palette;

    let mut csiv = Cursive::default();
    csiv.set_theme(theme);

    let meta = Metadata::load()?;
    if meta.get_token().is_empty() {
        return Err("User token not set!".into());
    }

    let cache_path = std::path::PathBuf::from("scoreboard.cache");
    let mut board = if cache_path.exists() {
        scoreboard::Scoreboard::load_cache(cache_path)?
    } else {
        scoreboard::Scoreboard::new()
    };

    for &pid in meta.problems() {
        board.add_problem(pid);
    }
    scoreboard::sync(&mut board, meta.get_token())?;
    board.save_cache("scoreboard.cache")?;

    let mut fterm = fake_term::FakeTerm::new();
    board.gen_table().print_term(&mut fterm)?;

    let mut view = TextView::new(fterm.into_inner()).no_wrap();
    csiv.add_layer(ScrollView::new(view).scroll_x(true).show_scrollbars(false));

    csiv.add_global_callback('q', |s| s.quit());
    csiv.add_global_callback('D', |s| s.toggle_debug_console());
    csiv.run();

    Ok(())
}
