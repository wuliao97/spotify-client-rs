use crate::constant::*;

use anyhow::{anyhow, Result};
use config_parser2::*;
use librespot_core::config::SessionConfig;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

static CONFIGS: OnceLock<Configs> = OnceLock::new();

#[derive(Debug)]
pub struct Configs {
    pub app_config: AppConfig,
    pub login_info: (String, String),
}

impl Configs {
    pub fn from_pass<T: Into<String>>(username: T, password: T) -> Self {
        Self {
            app_config: AppConfig::default(),
            login_info: (username.into(), password.into()),
        }
    }
}


impl Configs {
    pub fn new<P, T>(config_folder: P, username: T, password: T) -> Result<Self>
        where
            P: AsRef<Path>,
            T: Into<String>
    {
        Ok(Self {
            app_config: AppConfig::new(config_folder)?,
            login_info: (username.into(), password.into())
        })
    }

    // <P: AsRef<Path>>
    #[cfg(feature = "env-file")]
    pub fn from_env() -> Result<Self> {
        use std::env::var;
        dotenvy::dotenv().ok();

        let config_path = var("SPOTIFY_CONFIG_PATH").unwrap_or(".config/spotify-player".to_string());
        let username = var("SPOTIFY_USERNAME")?;
        let password = var("SPOTIFY_PASSWORD")?;

        Self::new(config_path, username, password)
    }
}

#[derive(Debug, Deserialize, Serialize, ConfigParse)]
/// Application configurations
pub struct AppConfig {
    pub client_id: String,
    pub client_port: u16,

    // session configs
    pub proxy: Option<String>,
    pub ap_port: Option<u16>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            // official Spotify web app's client id
            client_id: "65b708073fc0480ea92a077233ca87bd".to_string(),
            client_port: 8080,
            proxy: None,
            ap_port: None,
        }
    }
}


impl AppConfig {
    #[cfg(feature = "file")]
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let mut config = Self::default();
        if !config.parse_config_file(path.as_ref())? {
            config.write_config_file(path.as_ref())?
        }

        Ok(config)
    }

    #[cfg(not(feature = "file"))]
    pub fn new(_: impl AsRef<Path>) -> Result<Self> {
        let config = Self::default();
        Ok(config)
    }

    // parses configurations from an application config file in `path` folder,
    // then updates the current configurations accordingly.
    // returns false if no config file found and true otherwise
    #[cfg(feature = "file")]
    fn parse_config_file<P: AsRef<Path>>(&mut self, path: P) -> Result<bool> {
        let file_path = path.as_ref().join(APP_CONFIG_FILE);
        match std::fs::read_to_string(file_path) {
            Ok(content) => self
                .parse(toml::from_str::<toml::Value>(&content)?)
                .map(|_| true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    #[cfg(feature = "file")]
    fn write_config_file<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        toml::to_string_pretty(&self)
            .map_err(From::from)
            .and_then(|content| {
                std::fs::write(path.as_ref().join(APP_CONFIG_FILE), content)
                    .map_err(From::from)
            })
    }

    pub fn session_config(&self) -> SessionConfig {
        let proxy = self
            .proxy
            .as_ref()
            .and_then(|proxy| match Url::parse(proxy) {
                Err(err) => {
                    tracing::warn!("failed to parse proxy url {proxy}: {err:#}");
                    None
                }
                Ok(url) => Some(url),
            });
        SessionConfig {
            proxy,
            ap_port: self.ap_port,
            ..Default::default()
        }
    }
}

/// gets the application's configuration folder path
#[cfg(feature = "file")]
pub fn get_config_folder_path() -> Result<PathBuf> {
    match dirs_next::home_dir() {
        Some(home) => Ok(format!("./{}", DEFAULT_CONFIG_FOLDER).into()),
        None => Err(anyhow!("cannot find the folder")),
    }
}

#[cfg(feature = "file")]
/// gets the application's cache folder path
pub fn get_cache_folder_path() -> Result<PathBuf> {
    match dirs_next::home_dir() {
        Some(home) =>  Ok(format!("./{}", DEFAULT_CACHE_FOLDER).into()),
        None => Err(anyhow!("cannot find the folder")),
    }
}


#[inline(always)]
pub fn get_config() -> &'static Configs {
    CONFIGS.get().expect("configs is already initialized")
}

pub fn set_config(configs: Configs) {
    CONFIGS
        .set(configs)
        .expect("configs should be initialized only once")
}

