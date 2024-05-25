use std::ops::Deref;
use std::sync::Arc;

use crate::auth::AuthConfig;
use crate::constant::*;

use anyhow::Context as _;
use anyhow::Result;
use librespot_core::session::Session;
use rspotify::{
    http::Query,
    model::{FullPlaylist, Market, Page, SimplifiedPlaylist},
    prelude::*,
};
use serde::Deserialize;

mod spotify;


/// The application's Spotify client
pub struct Client {
    http: reqwest::Client,
    spotify: Arc<spotify::Spotify>,
    auth_config: AuthConfig,
}

impl Deref for Client {
    type Target = spotify::Spotify;
    fn deref(&self) -> &Self::Target {
        self.spotify.as_ref()
    }
}

fn market_query() -> Query<'static> {
    Query::from([("market", "from_token")])
}

impl Client {
    /// Construct a new client
    pub fn new(session: Session, auth_config: AuthConfig, client_id: String) -> Self {
        Self {
            spotify: Arc::new(spotify::Spotify::new(session, client_id)),
            http: reqwest::Client::new(),
            auth_config,
        }
    }

    /// Create a new client session
    // unused variables:
    // - `state` when the `streaming` feature is not enabled
    #[allow(unused_variables)]
    async fn new_session(&self) -> Result<()> {
        let session = crate::auth::new_session(&self.auth_config, false).await?;
        *self.session.lock().await = Some(session);

        tracing::info!("Used a new session for Spotify client.");

        Ok(())
    }

    /// Get the UserName of Spotify
    pub fn username(&self) -> UserId {
        let name: &str = self.auth_config.login_info.0.as_ref();
        UserId::from_id(name).unwrap()
    }

    /// Check if the current session is valid and if invalid, create a new session
    pub async fn check_valid_session(&self) -> Result<()> {
        if self.session().await.is_invalid() {
            tracing::info!("Client's current session is invalid, creating a new session...");
            self.new_session()
                .await
                .context("create new client session")?;
        }
        Ok(())
    }

    /// Get Spotify's available browse categories
    pub async fn browse_categories(&self) -> Result<Vec<Category>> {
        let first_page = self
            .categories_manual(Some("EN"), None, Some(50), None)
            .await?;

        Ok(first_page.items.into_iter().map(Category::from).collect())
    }

    /// Get Spotify's available browse playlists of a given category
    pub async fn browse_category_playlists(&self, category_id: &str) -> Result<Vec<Playlist>> {
        let first_page = self
            .category_playlists_manual(category_id, None, Some(50), None)
            .await?;

        Ok(first_page.items.into_iter().map(Playlist::from).collect())
    }

    /// Get the saved (liked) tracks of the current user
    pub async fn current_user_saved_tracks(&self) -> Result<Vec<Track>> {
        let first_page = self
            .current_user_saved_tracks_manual(Some(Market::FromToken), Some(50), None)
            .await?;
        let tracks = self.all_paging_items(first_page, &market_query()).await?;
        Ok(tracks
            .into_iter()
            .filter_map(|t| Track::try_from_full_track(t.track))
            .collect())
    }

    /// Get the recently played tracks of the current user
    pub async fn current_user_recently_played_tracks(&self) -> Result<Vec<Track>> {
        let first_page = self.current_user_recently_played(Some(50), None).await?;

        let play_histories = self.all_cursor_based_paging_items(first_page).await?;

        // de-duplicate the tracks returned from the recently-played API
        let mut tracks = Vec::<Track>::new();
        for history in play_histories {
            if !tracks.iter().any(|t| t.name == history.track.name) {
                if let Some(track) = Track::try_from_full_track(history.track) {
                    tracks.push(track);
                }
            }
        }
        Ok(tracks)
    }

    /// Get the top tracks of the current user
    pub async fn current_user_top_tracks(&self) -> Result<Vec<Track>> {
        let first_page = self
            .current_user_top_tracks_manual(None, Some(50), None)
            .await?;

        let tracks = self.all_paging_items(first_page, &Query::new()).await?;
        Ok(tracks
            .into_iter()
            .filter_map(Track::try_from_full_track)
            .collect())
    }

    /// Get all playlists of the current user
    pub async fn current_user_playlists(&self) -> Result<Vec<Playlist>> {
        // TODO: this should use `rspotify::current_user_playlists_manual` API instead of `internal_call`
        // See: https://github.com/ramsayleung/rspotify/issues/459
        let first_page = self
            .http_get::<Page<SimplifiedPlaylist>>(
                &format!("{SPOTIFY_API_ENDPOINT}/me/playlists"),
                &Query::from([("limit", "50")]),
            )
            .await?;
        // let first_page = self
        //     .current_user_playlists_manual(Some(50), None)
        //     .await?;

        let playlists = self.all_paging_items(first_page, &Query::new()).await?;
        Ok(playlists.into_iter().map(|p| p.into()).collect())
    }

    /// Get all followed artists of the current user
    pub async fn current_user_followed_artists(&self) -> Result<Vec<Artist>> {
        let first_page = self
            .spotify
            .current_user_followed_artists(None, None)
            .await?;

        // followed artists pagination is handled different from
        // other paginations. The endpoint uses cursor-based pagination.
        let mut artists = first_page.items;
        let mut maybe_next = first_page.next;
        while let Some(url) = maybe_next {
            let mut next_page = self
                .http_get::<rspotify_model::CursorPageFullArtists>(&url, &Query::new())
                .await?
                .artists;
            artists.append(&mut next_page.items);
            maybe_next = next_page.next;
        }

        // converts `rspotify_model::FullArtist` into `state::Artist`
        Ok(artists.into_iter().map(|a| a.into()).collect())
    }

    /// Get all saved albums of the current user
    pub async fn current_user_saved_albums(&self) -> Result<Vec<Album>> {
        let first_page = self
            .current_user_saved_albums_manual(Some(Market::FromToken), Some(50), None)
            .await?;

        let albums = self.all_paging_items(first_page, &Query::new()).await?;

        // converts `rspotify_model::SavedAlbum` into `state::Album`
        Ok(albums.into_iter().map(|a| a.album.into()).collect())
    }

    /// Get all albums of an artist
    pub async fn artist_albums(&self, artist_id: ArtistId<'_>) -> Result<Vec<Album>> {
        let payload = market_query();

        let mut singles = {
            let first_page = self
                .artist_albums_manual(
                    artist_id.as_ref(),
                    Some(rspotify_model::AlbumType::Single),
                    Some(Market::FromToken),
                    Some(50),
                    None,
                )
                .await?;
            self.all_paging_items(first_page, &payload).await
        }?;
        let mut albums = {
            let first_page = self
                .artist_albums_manual(
                    artist_id.as_ref(),
                    Some(rspotify_model::AlbumType::Album),
                    Some(Market::FromToken),
                    Some(50),
                    None,
                )
                .await?;
            self.all_paging_items(first_page, &payload).await
        }?;
        albums.append(&mut singles);

        // converts `rspotify_model::SimplifiedAlbum` into `state::Album`
        let albums = albums
            .into_iter()
            .filter_map(Album::try_from_simplified_album)
            .collect();
        Ok(self.process_artist_albums(albums))
    }

    /// Get recommendation (radio) tracks based on a seed
    pub async fn radio_tracks(&self, seed_uri: String) -> Result<Vec<Track>> {
        let session = self.session().await;

        // Get an autoplay URI from the seed URI.
        // The return URI is a Spotify station's URI
        let autoplay_query_url = format!("hm://autoplay-enabled/query?uri={seed_uri}");
        let response = session
            .mercury()
            .get(autoplay_query_url)
            .await
            .map_err(|_| anyhow::anyhow!("Failed to get autoplay URI: got a Mercury error"))?;
        if response.status_code != 200 {
            anyhow::bail!(
                "Failed to get autoplay URI: got non-OK status code: {}",
                response.status_code
            );
        }
        let autoplay_uri = String::from_utf8(response.payload[0].to_vec())?;

        // Retrieve radio's data based on the autoplay URI
        let radio_query_url = format!("hm://radio-apollo/v3/stations/{autoplay_uri}");
        let response = session.mercury().get(radio_query_url).await.map_err(|_| {
            anyhow::anyhow!("Failed to get radio data of {autoplay_uri}: got a Mercury error")
        })?;
        if response.status_code != 200 {
            anyhow::bail!(
                "Failed to get radio data of {autoplay_uri}: got non-OK status code: {}",
                response.status_code
            );
        }

        #[derive(Debug, Deserialize)]
        struct TrackData {
            original_gid: String,
        }
        #[derive(Debug, Deserialize)]
        struct RadioStationResponse {
            tracks: Vec<TrackData>,
        }
        // Parse a list consisting of IDs of tracks inside the radio station
        let track_ids = serde_json::from_slice::<RadioStationResponse>(&response.payload[0])?
            .tracks
            .into_iter()
            .filter_map(|t| TrackId::from_id(t.original_gid).ok());

        // Retrieve tracks based on IDs
        let tracks = self.tracks(track_ids, Some(Market::FromToken)).await?;
        let tracks = tracks
            .into_iter()
            .filter_map(Track::try_from_full_track)
            .collect();

        Ok(tracks)
    }

    /// Search for items (tracks, artists, albums, playlists) matching a given query
    pub async fn search(&self, query: &str) -> Result<SearchResults> {
        let (track_result, artist_result, album_result, playlist_result) = tokio::try_join!(
            self.search_specific_type(query, rspotify_model::SearchType::Track),
            self.search_specific_type(query, rspotify_model::SearchType::Artist),
            self.search_specific_type(query, rspotify_model::SearchType::Album),
            self.search_specific_type(query, rspotify_model::SearchType::Playlist)
        )?;

        let (tracks, artists, albums, playlists) = (
            match track_result {
                rspotify_model::SearchResult::Tracks(p) => p
                    .items
                    .into_iter()
                    .filter_map(Track::try_from_full_track)
                    .collect(),
                _ => anyhow::bail!("expect a track search result"),
            },
            match artist_result {
                rspotify_model::SearchResult::Artists(p) => {
                    p.items.into_iter().map(|a| a.into()).collect()
                }
                _ => anyhow::bail!("expect an artist search result"),
            },
            match album_result {
                rspotify_model::SearchResult::Albums(p) => p
                    .items
                    .into_iter()
                    .filter_map(Album::try_from_simplified_album)
                    .collect(),
                _ => anyhow::bail!("expect an album search result"),
            },
            match playlist_result {
                rspotify_model::SearchResult::Playlists(p) => {
                    p.items.into_iter().map(|i| i.into()).collect()
                }
                _ => anyhow::bail!("expect a playlist search result"),
            },
        );

        Ok(SearchResults {
            tracks,
            artists,
            albums,
            playlists,
        })
    }

    /// Search for items of a specific type matching a given query
    pub async fn search_specific_type(
        &self,
        query: &str,
        _type: rspotify_model::SearchType,
    ) -> Result<rspotify_model::SearchResult> {
        Ok(self
            .spotify
            .search(query, _type, None, None, None, None)
            .await?)
    }

    /// Add a track to a playlist
    pub async fn add_track_to_playlist(
        &self,
        playlist_id: PlaylistId<'_>,
        track_id: TrackId<'_>,
    ) -> Result<()> {
        // remove all the occurrences of the track to ensure no duplication in the playlist
        self.playlist_remove_all_occurrences_of_items(
            playlist_id.as_ref(),
            [PlayableId::Track(track_id.as_ref())],
            None,
        )
            .await?;

        self.playlist_add_items(
            playlist_id.as_ref(),
            [PlayableId::Track(track_id.as_ref())],
            None,
        )
            .await?;

        Ok(())
    }

    pub async fn add_tracks_to_playlist(
        &self
    ) -> Result<()> {

        Ok(())
    }

    /// Remove a track from a playlist
    pub async fn delete_track_from_playlist(
        &self,
        playlist_id: PlaylistId<'_>,
        track_id: TrackId<'_>,
    ) -> Result<()> {
        // remove all the occurrences of the track to ensure no duplication in the playlist
        self.playlist_remove_all_occurrences_of_items(
            playlist_id.as_ref(),
            [PlayableId::Track(track_id.as_ref())],
            None,
        )
            .await?;

        Ok(())
    }

    /// Reorder items in a playlist
    async fn reorder_playlist_items(
        &self,
        playlist_id: PlaylistId<'_>,
        insert_index: usize,
        range_start: usize,
        range_length: Option<usize>,
        snapshot_id: Option<&str>,
    ) -> Result<()> {
        let insert_before = match insert_index > range_start {
            true => insert_index + 1,
            false => insert_index,
        };

        self.playlist_reorder_items(
            playlist_id.clone(),
            Some(range_start as i32),
            Some(insert_before as i32),
            range_length.map(|range_length| range_length as u32),
            snapshot_id,
        )
            .await?;

        Ok(())
    }

    /// Get a playlist context data
    pub async fn playlist_context(&self, playlist_id: PlaylistId<'_>) -> Result<Context> {
        let playlist_uri = playlist_id.uri();
        tracing::info!("Get playlist context: {}", playlist_uri);

        // TODO: this should use `rspotify::playlist` API instead of `internal_call`
        // See: https://github.com/ramsayleung/rspotify/issues/459
        // let playlist = self
        //     .playlist(playlist_id, None, Some(Market::FromToken))
        //     .await?;
        let playlist = self
            .http_get::<FullPlaylist>(
                &format!("{SPOTIFY_API_ENDPOINT}/playlists/{}", playlist_id.id()),
                &market_query(),
            )
            .await?;

        // get the playlist's tracks
        let first_page = playlist.tracks.clone();
        let tracks = self
            .all_paging_items(first_page, &market_query())
            .await?
            .into_iter()
            .filter_map(|item| match item.track {
                Some(rspotify_model::PlayableItem::Track(track)) => {
                    Track::try_from_full_track(track)
                }
                _ => None,
            })
            .collect::<Vec<_>>();

        Ok(Context::Playlist {
            playlist: playlist.into(),
            tracks,
        })
    }

    /// Get an album context data
    pub async fn album_context(&self, album_id: AlbumId<'_>) -> Result<Context> {
        let album_uri = album_id.uri();
        tracing::info!("Get album context: {}", album_uri);

        let album = self.album(album_id, Some(Market::FromToken)).await?;
        let first_page = album.tracks.clone();

        // converts `rspotify_model::FullAlbum` into `state::Album`
        let album: Album = album.into();

        // get the album's tracks
        let tracks = self
            .all_paging_items(first_page, &Query::new())
            .await?
            .into_iter()
            .filter_map(|t| {
                // simplified track doesn't have album so
                // we need to manually include one during
                // converting into `state::Track`
                Track::try_from_simplified_track(t).map(|mut t| {
                    t.album = Some(album.clone());
                    t
                })
            })
            .collect::<Vec<_>>();

        Ok(Context::Album { album, tracks })
    }

    /// Get an artist context data
    pub async fn artist_context(&self, artist_id: ArtistId<'_>) -> Result<Context> {
        let artist_uri = artist_id.uri();
        tracing::info!("Get artist context: {}", artist_uri);

        // get the artist's information, including top tracks, related artists, and albums

        let artist = self.artist(artist_id.as_ref()).await?.into();

        let top_tracks = self
            .artist_top_tracks(artist_id.as_ref(), Some(Market::FromToken))
            .await?;
        let top_tracks = top_tracks
            .into_iter()
            .filter_map(Track::try_from_full_track)
            .collect::<Vec<_>>();

        let related_artists = self.artist_related_artists(artist_id.as_ref()).await?;
        let related_artists = related_artists
            .into_iter()
            .map(|a| a.into())
            .collect::<Vec<_>>();

        let albums = self.artist_albums(artist_id.as_ref()).await?;

        Ok(Context::Artist {
            artist,
            top_tracks,
            albums,
            related_artists,
        })
    }

    /// Make a GET HTTP request to the Spotify server
    async fn http_get<T>(&self, url: &str, payload: &Query<'_>) -> Result<T>
        where
            T: serde::de::DeserializeOwned,
    {
        /// a helper function to process an API response from Spotify server
        ///
        /// This function is mainly used to patch upstream API bugs , resulting in
        /// a type error when a third-party library like `rspotify` parses the response
        fn process_spotify_api_response(text: String) -> String {
            // See: https://github.com/ramsayleung/rspotify/issues/459
            text.replace("\"images\":null", "\"images\":[]")
        }

        let access_token = self.access_token().await?;

        tracing::debug!("{access_token} {url}");

        let response = self
            .http
            .get(url)
            .query(payload)
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {access_token}"),
            )
            .send()
            .await?;

        let text = process_spotify_api_response(response.text().await?);
        tracing::debug!("{text}");

        Ok(serde_json::from_str(&text)?)
    }

    /// Get all paging items starting from a pagination object of the first page
    async fn all_paging_items<T>(
        &self,
        first_page: rspotify_model::Page<T>,
        payload: &Query<'_>,
    ) -> Result<Vec<T>>
        where
            T: serde::de::DeserializeOwned,
    {
        let mut items = first_page.items;
        let mut maybe_next = first_page.next;

        while let Some(url) = maybe_next {
            let mut next_page = self
                .http_get::<rspotify_model::Page<T>>(&url, payload)
                .await?;
            items.append(&mut next_page.items);
            maybe_next = next_page.next;
        }
        Ok(items)
    }

    /// Get all cursor-based paging items starting from a pagination object of the first page
    async fn all_cursor_based_paging_items<T>(
        &self,
        first_page: rspotify_model::CursorBasedPage<T>,
    ) -> Result<Vec<T>>
        where
            T: serde::de::DeserializeOwned,
    {
        let mut items = first_page.items;
        let mut maybe_next = first_page.next;
        while let Some(url) = maybe_next {
            let mut next_page = self
                .http_get::<rspotify_model::CursorBasedPage<T>>(&url, &Query::new())
                .await?;
            items.append(&mut next_page.items);
            maybe_next = next_page.next;
        }
        Ok(items)
    }

    /// Create a new playlist
    async fn create_new_playlist(
        &self,
        user_id: UserId<'static>,
        playlist_name: &str,
        public: bool,
        collab: bool,
        desc: &str,
    ) -> Result<()> {
        let playlist: Playlist = self
            .user_playlist_create(
                user_id,
                playlist_name,
                Some(public),
                Some(collab),
                Some(desc),
            )
            .await?
            .into();
        tracing::info!(
            "new playlist (name={},id={}) was successfully created",
            playlist.name,
            playlist.id
        );

        Ok(())
    }


    /// Process a list of albums, which includes
    /// - sort albums by the release date
    /// - remove albums with duplicated names
    fn process_artist_albums(&self, albums: Vec<Album>) -> Vec<Album> {
        let mut albums = albums.into_iter().collect::<Vec<_>>();

        albums.sort_by(|x, y| x.release_date.partial_cmp(&y.release_date).unwrap());

        // use a HashSet to keep track albums with the same name
        let mut seen_names = std::collections::HashSet::new();

        albums.into_iter().rfold(vec![], |mut acc, a| {
            if !seen_names.contains(&a.name) {
                seen_names.insert(a.name.clone());
                acc.push(a);
            }
            acc
        })
    }
}
