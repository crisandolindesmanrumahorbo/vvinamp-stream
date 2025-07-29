use crate::model::GetTrack;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;

use request_http_parser::parser::Request;
use sqlx::{Pool, Postgres};
use tokio::net::TcpStream;

use crate::{
    constants::{NOT_FOUND, OK_RESPONSE},
    repo::Repository,
};

pub struct TrackService {}

impl TrackService {
    pub async fn query_track(
        mut socket: TcpStream,
        _request: Request,
        pool: Arc<Pool<Postgres>>,
    ) -> std::io::Result<()> {
        let tracks = match Repository::fetch_all_tracks(&pool).await {
            Ok(tracks) => tracks,
            Err(e) => {
                println!("{:?}", e);
                let _ = socket
                    .write_all(format!("{}{}", NOT_FOUND, "404 Not Found").as_bytes())
                    .await;
                return Ok(());
            }
        };
        let json: String = serde_json::to_string::<Vec<GetTrack>>(&tracks).expect("error serde");
        socket
            .write_all(format!("{}{}", OK_RESPONSE, json).as_bytes())
            .await
            .expect("Failed to write");
        Ok(())
    }
}
