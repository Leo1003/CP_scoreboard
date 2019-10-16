#![allow(non_snake_case)]

#[macro_use]
extern crate custom_error;
#[macro_use]
extern crate log;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate prettytable;

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
use cursive::views::{ScrollView, TextView};
use cursive::Cursive;
use std::error::Error;
use std::sync::Arc;
use term::Terminal as _;

lazy_static! {
    static ref CURSIVE_THEME: Theme = {
        let mut palette = Palette::default();
        palette[PaletteColor::Background] = Color::Dark(BaseColor::Black);
        palette[PaletteColor::Primary] = Color::Dark(BaseColor::White);
        palette[PaletteColor::View] = Color::Dark(BaseColor::Black);
        palette[PaletteColor::Shadow] = Color::Light(BaseColor::Black);
        let mut theme = Theme::default();
        theme.shadow = false;
        theme.palette = palette;
        theme
    };
}

fn sync_get_content(board: Arc<Scoreboard>, meta: &Metadata) -> SimpleResult<FakeTermString> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(
        board
            .clone()
            .fetch(meta.get_group(), meta.get_token().to_owned()),
    )?;

    board.save_cache("scoreboard.cache")?;
    let mut fterm = fake_term::FakeTerm::new();

    board.gen_table(meta.problems()).print_term(&mut fterm)?;
    Ok(fterm.into_inner())
}

fn show_content(content: FakeTermString) -> bool {
    let mut csiv = Cursive::default();
    csiv.set_theme(CURSIVE_THEME.clone());
    let view = TextView::new(content).no_wrap().with_id("table");
    csiv.add_fullscreen_layer(ScrollView::new(view).scroll_x(true).show_scrollbars(false));

    csiv.set_user_data(false);
    csiv.add_global_callback('q', |s| s.quit());
    csiv.add_global_callback('r', |s| {
        *s.user_data().unwrap() = true;
        s.quit();
    });
    csiv.run();
    csiv.take_user_data().unwrap()
}

fn main() -> Result<(), Box<dyn Error>> {
    let env = if cfg!(debug_assertions) {
        env_logger::Env::new().default_filter_or("FOJ_scoreboard=debug")
    } else {
        env_logger::Env::new().default_filter_or("FOJ_scoreboard=info")
    };
    env_logger::Builder::from_env(env).init();

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

    let mut running = true;
    while running {
        info!("Refreshing data. Please wait...");
        let content = sync_get_content(board.clone(), &meta)?;
        debug!("Showing content...");
        running = show_content(content);
    }

    Ok(())
}
