use std::sync::Arc;

use crate::constants::OPTIONS_CORS;
use crate::constants::{BAD_REQUEST, NOT_FOUND};
use crate::file::File;
use crate::hls::HlsService;
use crate::stream::Stream;
use crate::track::TrackService;
use anyhow::anyhow;
use anyhow::{Context, Result};
use request_http_parser::parser::{
    Method::GET, Method::HEAD, Method::OPTIONS, Method::POST, Request,
};
use sqlx::{Pool, Postgres};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot::Receiver;

pub struct Server {
    pub pool: Arc<Pool<Postgres>>,
}

impl Server {
    pub fn new(pool: Pool<Postgres>) -> Self {
        let pool = Arc::new(pool);
        Self { pool }
    }
    pub async fn start(&self, mut shutdown_rx: Receiver<()>) -> anyhow::Result<()> {
        let listener = TcpListener::bind("0.0.0.0:3001")
            .await
            .expect("failed to binding port");
        println!("Server running on http://0.0.0.0:3001");

        loop {
            tokio::select! {
                conn = listener.accept() => {
                    let ( stream, _) = conn?;

                    let pool = Arc::clone(&self.pool);

                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_client(stream,&pool).await {
                            eprintln!("Connection error: {}", e);
                        }
                    });
                }
                // Shutdown signal check
                _ = &mut shutdown_rx => {
                    println!("Shutting down server...");
                    break;
                }
            }
        }
        Ok(())
    }

    async fn handle_client(mut socket: TcpStream, pool: &Arc<Pool<Postgres>>) -> Result<()> {
        let mut buffer = [0; 1024];
        if let Ok(n) = socket.read(&mut buffer).await {
            let req_str = String::from_utf8_lossy(&buffer[..n]);
            println!("req:\n {}", req_str);
            let request = match Request::new(&req_str) {
                Ok(req) => req,
                Err(e) => {
                    println!("{}", e);
                    let _ = socket
                        .write_all(format!("{}{}", BAD_REQUEST, e).as_bytes())
                        .await
                        .context("Failed to write");

                    let _ = socket.flush().await.context("Failed to flush");
                    return Err(anyhow!("request format invalid"));
                }
            };
            println!("metod {:?} path {:?}", request.method, request.path);
            match (&request.method, request.path.as_str()) {
                (OPTIONS, _) => {
                    let _ = socket
                        .write_all(format!("{}{}", OPTIONS_CORS, "").as_bytes())
                        .await;
                }
                (HEAD, "/stream") => Stream::get_info(socket, request)
                    .await
                    .expect("error head stream"),
                (GET, "/stream") => Stream::stream_song(socket, request)
                    .await
                    .expect("error handle"),
                (POST, "/stream") => Stream::add_song(socket, request).await.expect("error add"),
                (GET, "/search") => Stream::search_song(socket, request)
                    .await
                    .expect("error search"),
                (GET, "/playlist") => HlsService::serve_hls_playlist1(socket, request)
                    .await
                    .expect("error handle"),
                (GET, "/segment") => HlsService::serve_hls_segment1(socket, request)
                    .await
                    .expect("error handle"),
                (POST, "/download") => File::download_task(socket, request, pool.clone())
                    .await
                    .expect("error downdload"),

                (GET, "/task-status") => File::get_task_status(socket, request)
                    .await
                    .expect("task status failed"),
                (GET, "/track") => TrackService::query_track(socket, request, pool.clone())
                    .await
                    .expect("track query failed"),
                _ => {
                    let _ = socket
                        .write_all(format!("{}{}", NOT_FOUND, "404 Not Found").as_bytes())
                        .await;
                }
            };

            Ok(())
        } else {
            println!("error ");
            Ok(())
        }
    }
}
