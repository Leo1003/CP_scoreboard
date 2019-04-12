pub type SimpleResult<T> = Result<T, SimpleError>;

custom_error! {pub SimpleError
    Request { source: reqwest::Error } = "HTTP Request Error",
    IO { source: std::io::Error } = "I/O Error",
    TomlSerialize { source: toml::ser::Error } = "TOML Serialize Error",
    TomlDeserialize { source: toml::de::Error } = "TOML Deserialize Error",
    Json { source: serde_json::error::Error } = "JSON Error",
    Custom { message: String } = "{message}",
}

impl From<&str> for SimpleError {
    fn from(err: &str) -> Self {
        SimpleError::Custom {
            message: format!("{}", err),
        }
    }
}
