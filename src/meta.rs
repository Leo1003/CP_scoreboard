use chrono::prelude::*;
use config::{Config, File, FileFormat};
use std::time::SystemTime;
use crate::error::{SimpleError, SimpleResult};

#[derive(Clone, Debug, PartialEq)]
pub struct Metadata {
    user_token: String,
    last_update: DateTime<Local>,
}

impl Metadata {
    pub fn load() -> SimpleResult<Self> {
        let mut cfg = Config::new();
        cfg.merge(File::new("meta", FileFormat::Toml))?;

        Ok(Self {
            user_token: cfg.get_str("user_token")?.to_owned(),
            last_update: cfg.get("last_update")?,
        })
    }

    pub fn get_token(&self) -> &str {
        &self.user_token
    }

    pub fn get_last_update(&self) -> &DateTime<Local> {
        &self.last_update
    }

    pub fn set_last_update(&mut self, time: &DateTime<Local>) {
        self.last_update = time.to_owned();
    }

    pub fn set_update_reset(&mut self) -> &DateTime<Local> {
        self.last_update = SystemTime::UNIX_EPOCH.into();
        self.get_last_update()
    }

    pub fn set_update_now(&mut self) -> &DateTime<Local> {
        self.last_update = Local::now();
        self.get_last_update()
    }

    pub fn updated(&self) -> bool {
        self.last_update > SystemTime::UNIX_EPOCH.into()
    }

    pub fn save(&self) -> SimpleResult<()> {
        unimplemented!();
    }
}

impl Default for Metadata {
    fn default() -> Self {
        Self {
            user_token: String::new(),
            last_update: SystemTime::UNIX_EPOCH.into(),
        }
    }
}
