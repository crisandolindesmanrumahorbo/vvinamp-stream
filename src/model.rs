use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct YtSearchResult {
    pub title: String,
    pub fulltitle: String,
    pub view_count: Option<u64>,
    pub duration: Option<u64>,
    pub duration_string: Option<String>,
    pub upload_date: Option<String>,
    pub channel: Option<String>,
    pub channel_follower_count: Option<u64>,
    pub like_count: Option<u64>,
    pub channel_is_verified: Option<bool>,
    pub thumbnails: Option<Vec<Thumbnail>>,
    pub thumbnail: String,
    pub webpage_url: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Thumbnail {
    pub url: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AddStream {
    pub title: String,
    pub youtube_url: String,
    pub start: Option<u32>,
    pub end: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatus {
    pub task_id: String,
    pub title: String,
    pub status: String, // downloading, converting, done, failed
    pub progress: u8,   // 0 to 100
    pub log: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, sqlx::FromRow)]
pub struct Track {
    pub track_id: Option<i32>,
    pub title: String,
    pub duration: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, sqlx::FromRow, Debug)]
pub struct GetTrack {
    pub title: String,
    pub duration: String,
}
