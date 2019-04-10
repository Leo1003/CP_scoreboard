use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub struct SimpleError {
    message: String,
}

impl fmt::Display for SimpleError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Error for SimpleError {}

impl From<config::ConfigError> for SimpleError {
    fn from(err: config::ConfigError) -> Self {
        Self {
            message: format!("{}", err)
        }
    }
}
