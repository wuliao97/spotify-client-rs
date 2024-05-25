pub use crate::model::*;
use once_cell::sync::Lazy;

pub static USER_TOP_TRACKS_ID: Lazy<TracksId> =
    Lazy::new(|| TracksId::new("tracks:user-top-tracks", "Top Tracks"));

pub static USER_RECENTLY_PLAYED_TRACKS_ID: Lazy<TracksId> = Lazy::new(|| {
    TracksId::new(
        "tracks:user-recently-played-tracks",
        "Recently Played Tracks",
    )
});

pub static USER_LIKED_TRACKS_ID: Lazy<TracksId> =
    Lazy::new(|| TracksId::new("tracks:user-liked-tracks", "Liked Tracks"));


pub const DEFAULT_CONFIG_FOLDER: &str = ".config/spotify-player";
pub const DEFAULT_CACHE_FOLDER: &str = ".cache/spotify-player";
pub const APP_CONFIG_FILE: &str = "app.toml";
pub const SPOTIFY_API_ENDPOINT: &str = "https://api.spotify.com/v1";
