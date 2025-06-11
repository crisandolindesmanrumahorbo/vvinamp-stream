use anyhow::anyhow;
use anyhow::{Context, Result};
use request_http_parser::parser::{Method::GET, Method::HEAD, Method::OPTIONS, Request};
use std::io::SeekFrom;
use tokio::fs::{self, File};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::net::TcpListener;

pub const BAD_REQUEST: &str = "HTTP/1.1 400 Bad Request\r\n\r\n";
pub const NOT_FOUND: &str = "HTTP/1.1 404 NOT FOUND\r\n\r\n";
pub const OPTIONS_CORS: &str = "HTTP/1.1 204 No Content\r\n\
            Access-Control-Allow-Origin: *\r\n\
            Access-Control-Allow-Methods: POST, GET, OPTIONS, HEAD\r\n\
            Access-Control-Allow-Headers: Content-Type\r\n\
            Access-Control-Max-Age: 86400\r\n\
            \r\n";
pub const OK_RESPONSE: &str = "HTTP/1.1 200 OK\r\n\
            Access-Control-Allow-Origin: *\r\n\
            Access-Control-Allow-Methods: POST, GET, OPTIONS, HEAD\r\n\
            Access-Control-Allow-Headers: Content-Type\r\n\
            Access-Control-Max-Age: 86400\r\n\
            Content-Type: application/json\r\n\
            \r\n";

#[tokio::main]
async fn main() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:3001").await?;
    println!("Listening on http://127.0.0.1:3001");

    loop {
        let (mut socket, _) = listener.accept().await?;

        tokio::spawn(async move {
            let mut buffer = [0; 1024];
            if let Ok(n) = socket.read(&mut buffer).await {
                let req_str = String::from_utf8_lossy(&buffer[..n]);
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
                match (&request.method, request.path.as_str()) {
                    (OPTIONS, _) => {
                        let _ = socket
                            .write_all(format!("{}{}", OPTIONS_CORS, "").as_bytes())
                            .await;
                    }
                    (HEAD, "/stream") => {
                        get_info(socket, request).await.expect("error head stream")
                    }
                    (GET, "/stream") => stream_song(socket, request).await.expect("error handle"),
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
        });
    }
}

async fn get_info(mut socket: tokio::net::TcpStream, request: Request) -> std::io::Result<()> {
    let song = match &request.params {
        Some(params) => match params.get("song") {
            Some(song) => song,
            None => {
                let _ = socket
                    .write_all(format!("{}{}", NOT_FOUND, "404 Not Found").as_bytes())
                    .await;
                return Ok(());
            }
        },
        None => {
            let _ = socket
                .write_all(format!("{}{}", NOT_FOUND, "404 Not Found").as_bytes())
                .await;
            return Ok(());
        }
    };
    let path = format!("./mp3/{}.mp3", song);
    let file = match File::open(path).await {
        Ok(file) => file,
        Err(e) => {
            println!("{}", e);
            let _ = socket
                .write_all(format!("{}{}", NOT_FOUND, "404 Not Found").as_bytes())
                .await;
            return Ok(());
        }
    };
    let metadata = file.metadata().await?;
    let total_size = metadata.len();
    let headers = format!(
        "HTTP/1.1 200 OK\r\n\
         Content-Length: {}\r\n\
         Accept-Ranges: bytes\r\n\
         Content-Type: audio/mpeg\r\n\
         Access-Control-Allow-Origin: *\r\n\
         \r\n",
        total_size
    );
    socket
        .write_all(headers.as_bytes())
        .await
        .expect("error write head");
    Ok(())
}

async fn stream_song(mut socket: tokio::net::TcpStream, request: Request) -> std::io::Result<()> {
    let song = match &request.params {
        Some(params) => match params.get("song") {
            Some(song) => song,
            None => {
                println!("no song params");
                let _ = socket
                    .write_all(format!("{}{}", NOT_FOUND, "404 Not Found").as_bytes())
                    .await;
                return Ok(());
            }
        },
        None => {
            println!("no params");

            let _ = socket
                .write_all(format!("{}{}", NOT_FOUND, "404 Not Found").as_bytes())
                .await;
            return Ok(());
        }
    };
    let path = format!("./mp3/{}.mp3", song);
    let metadata = fs::metadata(&path).await?;
    let file_size = metadata.len();

    let range = match request.headers.get("range") {
        Some(range) => range,
        None => {
            println!("no range header");

            let _ = socket
                .write_all(format!("{}{}", BAD_REQUEST, "").as_bytes())
                .await;

            return Ok(());
        }
    };
    let (start, end) = match range.split("=").nth(1) {
        Some(parts) => {
            let parts: Vec<&str> = parts.trim().split('-').collect();
            let start = match parts[0].parse::<u64>() {
                Ok(start) => start,
                Err(_) => {
                    println!("start");

                    let _ = socket
                        .write_all(format!("{}{}", BAD_REQUEST, "").as_bytes())
                        .await;

                    return Ok(());
                }
            };
            let end = match parts[1].parse::<u64>() {
                Ok(end) => end,
                Err(_) => {
                    let default_chunk_size: u64 = 64 * 1024;
                    let max_end = start + default_chunk_size - 1;
                    std::cmp::min(max_end, file_size - 1)
                }
            };
            (start, end)
        }
        None => {
            println!("value range ");

            let _ = socket
                .write_all(format!("{}{}", BAD_REQUEST, "").as_bytes())
                .await;
            return Ok(());
        }
    };
    let content_length = end - start + 1;
    let mut file = match File::open(path).await {
        Ok(file) => file,
        Err(e) => {
            println!("{}", e);
            let _ = socket
                .write_all(format!("{}{}", NOT_FOUND, "404 Not Found").as_bytes())
                .await;
            return Ok(());
        }
    };
    file.seek(SeekFrom::Start(start)).await?;
    let metadata = file.metadata().await?;
    let file_size = metadata.len();

    let mut buf = vec![0; content_length as usize];
    file.read_exact(&mut buf).await?;

    let header = format!(
        "HTTP/1.1 206 Partial Content\r\n\
        Content-Type: audio/mpeg\r\n\
        Content-Length: {}\r\n\
        Accept-Ranges: bytes\r\n\
        Access-Control-Allow-Origin: *\r\n\
        Content-Range: bytes {}-{}/{}\r\n\
        \r\n",
        content_length, start, end, file_size
    );
    socket.write_all(header.as_bytes()).await?;
    socket.write_all(&buf).await?;

    Ok(())
}
