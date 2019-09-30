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
extern crate tokio;
extern crate tokio_timer;
extern crate toml;
#[macro_use]
extern crate log;
extern crate futures;

mod api;
mod error;
mod fake_term;
mod meta;
mod scoreboard;

use self::error::SimpleResult;
use self::fake_term::FakeTermString;
use self::meta::Metadata;
use self::scoreboard::Scoreboard;
use cursive::theme::*;
use cursive::traits::Identifiable;
use cursive::view::Selector;
use cursive::views::{DebugView, Dialog, ScrollView, TextView};
use cursive::Cursive;
use log::LevelFilter;
use std::error::Error;
use std::sync::Arc;
use term::Terminal as _;
use tokio_timer::clock::Clock;

fn sync_get_content(board: Arc<Scoreboard>, meta: &Metadata) -> SimpleResult<FakeTermString> {
    let mut runtime = tokio::runtime::Builder::new().clock(Clock::new()).build()?;
    runtime.block_on(scoreboard::sync(
        board.clone(),
        meta.get_group(),
        meta.get_token().to_owned(),
    ))?;

    board.save_cache("scoreboard.cache")?;
    let mut fterm = fake_term::FakeTerm::new();

    board.gen_table(meta.problems()).print_term(&mut fterm)?;
    Ok(fterm.into_inner())
}

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
    cursive::logger::init();
    if cfg!(debug_assertions) {
        log::set_max_level(LevelFilter::Debug);
    } else {
        log::set_max_level(LevelFilter::Info);
    }
    csiv.add_layer(DebugView::new());

    let meta = Metadata::load()?;
    if meta.get_token().is_empty() {
        return Err("User token not set!".into());
    }

    let cache_path = std::path::PathBuf::from("scoreboard.cache");
    let board = if cache_path.exists() {
        Scoreboard::load_cache(cache_path)?
    } else {
        Scoreboard::new()
    };

    let board = Arc::new(board);
    let content = sync_get_content(board.clone(), &meta)?;

    csiv.pop_layer();
    let view = TextView::new(content).no_wrap().with_id("table");
    csiv.add_fullscreen_layer(ScrollView::new(view).scroll_x(true).show_scrollbars(false));

    csiv.add_global_callback('q', |s| s.quit());
    csiv.add_global_callback('D', |s| s.toggle_debug_console());
    csiv.add_global_callback('r', move |s| {
        let board = board.clone();
        s.add_layer(
            Dialog::text("Refreshing data. Please wait...")
                .title("Refreshing")
                .with_id("refr_dlg"),
        );
        s.focus(&Selector::Id("refr_dlg")).unwrap();
        s.refresh();
        if s.call_on(
            &Selector::Id("table"),
            |table_view: &mut TextView| match sync_get_content(board, &meta) {
                Ok(content) => {
                    table_view.set_content(content);
                    Ok(())
                }
                Err(e) => {
                    error!("{}", e);
                    Err(e)
                }
            },
        )
        .unwrap()
        .is_err()
        {
            s.show_debug_console();
        }
        s.pop_layer();
    });
    csiv.run();

    Ok(())
}
