use std::error::Error;
use std::fmt;

pub type SimpleResult<T> = Result<T, SimpleError>;

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

impl From<reqwest::Error> for SimpleError {
    fn from(err: reqwest::Error) -> Self {
        Self {
            message: format!("{}", err)
        }
    }
}

impl From<cookie::ParseError> for SimpleError {
    fn from(err: cookie::ParseError) -> Self {
        Self {
            message: format!("{}", err)
        }
    }
}

impl From<serde_json::Error> for SimpleError {
    fn from(err: serde_json::Error) -> Self {
        Self {
            message: format!("{}", err)
        }
    }
}

impl From<&str> for SimpleError {
    fn from(err: &str) -> Self {
        Self {
            message: format!("{}", err)
        }
    }
}
