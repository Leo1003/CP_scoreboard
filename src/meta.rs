use crate::error::SimpleResult;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::ErrorKind;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Metadata {
    group_id: u32,
    user_token: String,
    problem_list: Option<Vec<u32>>,
}

impl Metadata {
    pub fn load() -> SimpleResult<Self> {
        let config_str = match fs::read_to_string("meta.toml") {
            Ok(string) => string,
            Err(e) => {
                if e.kind() == ErrorKind::NotFound {
                    let def_meta = Self::default();
                    def_meta.save()?;
                    eprintln!("Meta file not found. A default meta has been generated.");
                }
                return Err(e.into());
            }
        };
        Ok(toml::from_str(&config_str)?)
    }

    pub fn get_group(&self) -> u32 {
        self.group_id
    }

    pub fn get_token(&self) -> &str {
        &self.user_token
    }

    pub fn problems(&self) -> Option<&[u32]> {
        self.problem_list.as_ref().and_then(|p| {
            if p.is_empty() {
                None
            } else {
                Some(p.as_slice())
            }
        })
    }

    pub fn save(&self) -> SimpleResult<()> {
        let config_str = toml::to_string_pretty(self)?;
        fs::write("meta.toml", config_str)?;
        Ok(())
    }
}
