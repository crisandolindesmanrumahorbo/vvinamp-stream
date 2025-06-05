use anyhow::anyhow;
use anyhow::{Context, Result};
use request_http_parser::parser::{Method::GET, Request};
use std::time::Duration;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

pub const BAD_REQUEST: &str = "HTTP/1.1 400 Bad Request\r\n\r\n";
pub const NOT_FOUND: &str = "HTTP/1.1 404 NOT FOUND\r\n\r\n";

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

async fn stream_song(mut socket: tokio::net::TcpStream, request: Request) -> std::io::Result<()> {
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

    let mut reader = BufReader::new(file);
    let headers = format!(
        concat!(
            "HTTP/1.1 200 OK\r\n",
            "Content-Type: audio/mpeg\r\n",
            "Transfer-Encoding: chunked\r\n",
            "Access-Control-Allow-Origin: *\r\n",
            "Access-Control-Expose-Headers: X-Content-Length\r\n",
            "X-Content-Length: {}\r\n",
            "\r\n"
        ),
        total_size
    );
    socket.write_all(headers.as_bytes()).await?;

    let mut buffer = [0u8; 32 * 1024];
    loop {
        let n = reader.read(&mut buffer).await?;
        if n == 0 {
            break;
        }

        let chunk = &buffer[..n];
        let size_line = format!("{:X}\r\n", chunk.len());
        socket.write_all(size_line.as_bytes()).await?;
        socket.write_all(chunk).await?;
        socket.write_all(b"\r\n").await?;

        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    socket.write_all(b"0\r\n\r\n").await?;
    Ok(())
}

