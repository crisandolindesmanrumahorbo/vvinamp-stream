use sqlx::{Pool, Postgres};

use crate::model::{GetTrack, Track};

pub struct Repository {}

impl Repository {
    pub async fn insert_track(
        new_track: &Track,
        pool: &Pool<Postgres>,
    ) -> Result<i32, anyhow::Error> {
        let row: (i32,) = match sqlx::query_as(
            r#"
            INSERT INTO tracks (title, duration, created_at) 
            VALUES ($1, $2, $3) 
            RETURNING track_id"#,
        )
        .bind(&new_track.title)
        .bind(&new_track.duration)
        .bind(&new_track.created_at)
        .fetch_one(pool)
        .await
        {
            Ok(row) => row,
            Err(e) => {
                anyhow::bail!("failed insert {}", e)
            }
        };
        Ok(row.0)
    }

    pub async fn fetch_all_tracks(pool: &Pool<Postgres>) -> Result<Vec<GetTrack>, anyhow::Error> {
        let tracks = sqlx::query_as::<_, GetTrack>(r#"SELECT title, duration FROM tracks"#)
            .fetch_all(pool)
            .await?;
        Ok(tracks)
    }
}
