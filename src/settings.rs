use std::env;
use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
#[allow(unused)]
pub struct Settings {
    /// hostname of this rendering server
    pub hostname: String,
    /// port to listen on
    pub port: usize,
    /// Path to the CA certificate
    pub ca_cert_path: String,
    /// Path to the client certificate
    pub client_cert_path: String,
    /// Path to the clients certificate key
    pub client_key_path: String,
    /// Path to the revocation list
    pub revocation_list_path: String,
    /// Path to the folder where templates data are stored temporarily. Gets cleared on start
    pub temp_template_path: String,
    /// Max concurrent rendering threads
    pub max_rendering_threads: u64,
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