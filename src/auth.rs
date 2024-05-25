use std::io::Write;

use anyhow::{anyhow, Result};
use dotenvy::var;
use librespot_core::{
    authentication::Credentials,
    cache::Cache,
    config::SessionConfig,
    session::{Session, SessionError},
};

use crate::config;
use crate::config::Configs;

#[derive(Clone)]
pub struct AuthConfig {
    pub cache: Cache,
    pub session_config: SessionConfig,
    pub login_info: (String, String)
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            cache: Cache::new(None::<String>, None, None, None).unwrap(),
            session_config: SessionConfig::default(),
            login_info: ("".to_string(), "".to_string()),
        }
    }
}

impl AuthConfig {
    #[cfg(not(feature = "file"))]
    pub fn new(configs: &Configs) -> Result<AuthConfig> {
        Ok(Self {
            cache: Cache::new(None::<String>, None, None, None).unwrap(),
            session_config: SessionConfig::default(),
            login_info: configs.login_info.to_owned(),
        })
    }

    #[cfg(feature = "file")]
    pub fn new(configs: &Configs) -> Result<AuthConfig> {
        let cache = Cache::new(
            None,
            None,
            None,
            None,
        )?;

        Ok(AuthConfig {
            cache,
            session_config: configs.app_config.session_config(),
            login_info: configs.login_info.to_owned(),
        })
    }
}


#[cfg(feature = "env-file")]
fn user_auth_details_from_env() -> Result<(String, String)> {
    use std::env::var;
    dotenvy::dotenv().ok();

    let username = var("SPOTIFY_USERNAME")?;
    let password = var("SPOTIFY_PASSWORD")?;

    Ok((username, password))
}

#[cfg(feature = "env-file")]
pub async fn new_session_with_new_creds(auth_config: &AuthConfig) -> Result<Session> {
    tracing::info!("Creating a new session with new authentication credentials");

    let mut user: Option<String> = None;

    let (username, password) = user_auth_details_from_env()?;

    user = Some(username.clone());
    match Session::connect(
        auth_config.session_config.clone(),
        Credentials::with_password(username, password),
        Some(auth_config.cache.clone()),
        true,
    )
        .await
    {
        Ok((session, _)) => {
            println!("Successfully authenticated as {}", user.unwrap_or_default());
            Ok(session)
        }
        Err(err) => {
            eprintln!("Failed to authenticate.");
            anyhow::bail!("Failed to authenticate: {err:#}")
        }
    }
}

#[cfg(feature = "env-file")]
pub async fn new_session(auth_config: &AuthConfig, reauth: bool) -> Result<Session> {
    match auth_config.cache.credentials() {
        None => {
            let msg = "No cached credentials found, please authenticate the application first.";
            if reauth {
                eprintln!("{msg}");
                new_session_with_new_creds(auth_config).await
            } else {
                anyhow::bail!(msg);
            }
        }
        Some(creds) => {
            match Session::connect(
                auth_config.session_config.clone(),
                creds,
                Some(auth_config.cache.clone()),
                true,
            )
                .await
            {
                Ok((session, _)) => {
                    tracing::info!(
                        "Successfully used the cached credentials to create a new session!"
                    );
                    Ok(session)
                }
                Err(err) => match err {
                    SessionError::AuthenticationError(err) => {
                        anyhow::bail!("Failed to authenticate using cached credentials: {err:#}");
                    }
                    SessionError::IoError(err) => {
                        anyhow::bail!("{err:#}\nPlease check your internet connection.");
                    }
                },
            }
        }
    }
}

#[cfg(not(feature = "env-file"))]
pub async fn new_session(auth_config: &AuthConfig, reauth: bool) -> Result<Session> {
    let (username, password) = auth_config.login_info.to_owned();
    let user = username.clone();

    match Session::connect(
        auth_config.session_config.clone(),
        Credentials::with_password(username, password),
        Some(auth_config.cache.clone()),
        true,
    )
        .await
    {
        Ok((session, _)) => {
            println!("Successfully authenticated as {}", user);
            Ok(session)
        }
        Err(err) => {
            eprintln!("Failed to authenticate.");
            anyhow::bail!("Failed to authenticate: {err:#}")
        }
    }
}