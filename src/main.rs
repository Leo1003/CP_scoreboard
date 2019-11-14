#![allow(non_snake_case)]

#[macro_use]
extern crate anyhow;
#[macro_use]
extern crate log;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate prettytable;

mod api;
mod fake_term;
mod meta;
mod scoreboard;

use self::fake_term::FakeTermString;
use self::meta::*;
use self::scoreboard::Scoreboard;
use anyhow::Result as AnyResult;
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

async fn sync_get_content(board: Arc<Scoreboard>, meta: &Metadata) -> AnyResult<FakeTermString> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(
        board
            .clone()
            .fetch(meta.get_group(), meta.get_token().to_owned()),
    )?;

    board.save_cache("scoreboard.cache").await?;
    let mut fterm = fake_term::FakeTerm::new();

    let mut probset = board.probset();
    if let Some(metaprob) = meta.problems() {
        probset = match meta.list_type() {
            ListType::BlackList => &probset - metaprob,
            ListType::WhiteList => &probset & metaprob,
        };
    }

    board.gen_table(&probset).print_term(&mut fterm)?;
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

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Setup Logger
    let env = if cfg!(debug_assertions) {
        env_logger::Env::new().default_filter_or("FOJ_scoreboard=debug")
    } else {
        env_logger::Env::new().default_filter_or("FOJ_scoreboard=info")
    };
    env_logger::Builder::from_env(env).init();

    // Load Metadata
    let meta = Metadata::load().await?;
    trace!("Loaded metadata: {:?}", &meta);
    if meta.get_token().is_empty() {
        return Err("User token not set!".into());
    }

    // Load Board Cache
    let cache_path = std::path::PathBuf::from("scoreboard.cache");
    let board = if cache_path.exists() {
        if log_enabled!(log::Level::Debug) {
            debug!("Found cache file: {:?}", cache_path.canonicalize());
        }
        Scoreboard::load_cache(cache_path).await?
    } else {
        debug!("Cache not found, creating a new one...");
        Scoreboard::new()
    };

    // Wrap in Arc
    let board = Arc::new(board);

    // Refresh Loop
    let mut refreshing = true;
    while refreshing {
        info!("Refreshing data. Please wait...");
        let content = sync_get_content(board.clone(), &meta).await?;
        debug!("Showing content...");
        refreshing = show_content(content);
    }

    Ok(())
}
