mod token;
mod utils;
mod constant;
mod config;
mod auth;
mod model;
mod client;

pub mod require {
    pub use crate::config::{Configs, get_config, set_config};
    pub use crate::client::Client;
    pub use crate::ClientHandler;
    pub use rspotify::clients::BaseClient as _;
    pub use rspotify::clients::OAuthClient as _;
}

pub mod prelude {
    pub use super::require::*;
    pub use rspotify::prelude::*;
    pub use rspotify::model::*;
}


pub struct ClientHandler {
    config: auth::AuthConfig
}

impl ClientHandler {
    pub fn new() -> Self {
        let auth_config = auth::AuthConfig::default();
        Self {
            config: auth_config,
        }
    }

    pub async fn client_new(&mut self, configs: &config::Configs) -> anyhow::Result<client::Client> {
        use rspotify::clients::BaseClient as _;

        let auth_config = auth::AuthConfig::new(configs)?;
        let session = auth::new_session(&auth_config, true).await?;
        let inner = client::Client::new(session, auth_config.to_owned(), configs.app_config.client_id.to_owned());
        inner.refresh_token().await?;

        self.config = auth_config;

        Ok(inner)
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use prelude::*;

    #[tokio::test]
    async fn it_works() -> anyhow::Result<()> {
        let config =  &Configs::from_pass("", "");
        let mut handler = ClientHandler::new();
        let client = handler.client_new(config).await?;
        let track_id = TrackId::from_id("6D6Pybzey0shI8U9ttRAPx")?;
        let result = client.track(track_id, None).await?;

        dbg!(result);

        Ok(())
    }
}
