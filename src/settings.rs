use std::env;
use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
#[allow(unused)]
pub struct Settings {
    pub hostname: String,
    pub port: usize,
    pub ca_cert_path: String,
    pub client_cert_path: String,
    pub client_key_path: String,
    pub revocation_list_path: String,
}

impl Settings{
    pub fn new() -> Result<Self, ConfigError>{
        let run_mode = env::var("RUN_MODE").unwrap_or_else(|_| "development".into());

        let s = Config::builder().add_source(File::with_name("config/default"))
            .add_source( File::with_name(&format!("config/{}", run_mode))
                             .required(false),)
            .add_source(File::with_name("config/local").required(false))
            .add_source(Environment::with_prefix("app"))
            .build()?;

        s.try_deserialize()
    }
}