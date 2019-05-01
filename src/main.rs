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
use cursive::views::{DebugView, ScrollView, TextView, Dialog};
use cursive::Cursive;
use log::LevelFilter;
use std::error::Error;
use term::Terminal as _;

fn sync_get_content(board: &mut Scoreboard, meta: &Metadata) -> SimpleResult<FakeTermString> {
    scoreboard::sync(board, meta.get_token())?;
    board.save_cache("scoreboard.cache")?;
    let mut fterm = fake_term::FakeTerm::new();
    board.gen_table().print_term(&mut fterm)?;
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
    let mut board = if cache_path.exists() {
        Scoreboard::load_cache(cache_path)?
    } else {
        Scoreboard::new()
    };
    for &pid in meta.problems() {
        board.add_problem(pid);
    }

    let content = sync_get_content(&mut board, &meta)?;

    csiv.pop_layer();
    let view = TextView::new(content).no_wrap().with_id("table");
    csiv.add_fullscreen_layer(ScrollView::new(view).scroll_x(true).show_scrollbars(false));

    csiv.add_global_callback('q', |s| s.quit());
    csiv.add_global_callback('D', |s| s.toggle_debug_console());
    csiv.add_global_callback('r', move |s| {
        s.add_layer(Dialog::text("Refreshing data. Please wait...").title("Refreshing").with_id("refr_dlg"));
        s.focus(&Selector::Id("refr_dlg")).unwrap();
        s.refresh();
        if let Err(_) = s
            .call_on(
                &Selector::Id("table"),
                |table_view: &mut TextView| match sync_get_content(&mut board, &meta) {
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
        {
            s.show_debug_console();
        }
        s.pop_layer();
    });
    csiv.run();

    Ok(())
}
