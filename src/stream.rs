use crate::constants::BAD_REQUEST;
use crate::constants::NOT_FOUND;
use crate::constants::OK_RESPONSE;
use crate::model::AddStream;
use crate::model::YtSearchResult;
use request_http_parser::parser::Request;
use std::path::Path;
use std::process::Command;
use tokio::fs::{self, File};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, SeekFrom};

pub struct Stream {}

impl Stream {
    pub async fn add_song(
        mut socket: tokio::net::TcpStream,
        request: Request,
    ) -> anyhow::Result<()> {
        let body_str = match request.body {
            Some(body) => body,
            None => {
                let _ = socket
                    .write_all(format!("{}{}", NOT_FOUND, "404 Not Found").as_bytes())
                    .await;
                return Ok(());
            }
        };
        let body: AddStream = match serde_json::from_str::<AddStream>(&body_str) {
            Ok(body) => body,
            Err(_) => {
                let _ = socket
                    .write_all(format!("{}{}", NOT_FOUND, "404 Not Found").as_bytes())
                    .await;
                return Ok(());
            }
        };

        let output_dir = Path::new("./mp3");
        std::fs::create_dir_all(&output_dir).expect("create dir error");

        let mut args = vec![
            body.youtube_url,
            "--extract-audio".to_string(),
            "--audio-format".to_string(),
            "mp3".to_string(),
            "--output".to_string(),
            "./mp3/%(title)s.%(ext)s".to_string(),
        ];

        if let Some(start_time) = body.start {
            args.push("--postprocessor-args".to_string());
            args.push(format!("-ss {}", start_time));
        }

        if let Some(end_time) = body.end {
            args.push("--postprocessor-args".to_string());
            args.push(format!("-to {}", end_time));
        }

        let status = Command::new("yt-dlp")
            .args(&args)
            .status()
            .expect("error status");

        println!("{:?}", status);

        if !status.success() {
            anyhow::bail!("yt-dlp download failed");
        }

        //save to db the song info so we can get all the list to FE
        socket
            .write_all(format!("{}{}", OK_RESPONSE, "Succeed add to server").as_bytes())
            .await
            .expect("Failed to write");
        Ok(())
    }

    pub async fn search_song(
        mut socket: tokio::net::TcpStream,
        request: Request,
    ) -> anyhow::Result<()> {
        let title = match &request.params {
            Some(params) => match params.get("title") {
                Some(title) => title,
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
        let decoded_title = percent_encoding::percent_decode(title.as_bytes())
            .decode_utf8()
            .expect("error decode percent");

        println!("title: {} after {}", title, decoded_title);

        let ytsearch_arg = format!("ytsearch10:\"{}\"", decoded_title);

        let output = Command::new("yt-dlp")
            .args([&ytsearch_arg, "--dump-json"])
            .output()?;

        if !output.status.success() {
            anyhow::bail!("yt-dlp error: {}", String::from_utf8_lossy(&output.stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut results = Vec::new();
        for line in stdout.lines() {
            if let Ok(result) = serde_json::from_str::<YtSearchResult>(line) {
                results.push(result);
            }
        }
        let json: String =
            serde_json::to_string::<Vec<YtSearchResult>>(&results).expect("error serde");
        socket
            .write_all(format!("{}{}", OK_RESPONSE, json).as_bytes())
            .await
            .expect("Failed to write");
        Ok(())
    }

    pub async fn get_info(
        mut socket: tokio::net::TcpStream,
        request: Request,
    ) -> std::io::Result<()> {
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
            "HTTP/1.1 204 No Content\r\n\
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

    pub async fn stream_song(
        mut socket: tokio::net::TcpStream,
        request: Request,
    ) -> std::io::Result<()> {
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

        // Check if Range header exists
        let (start, end, is_range_request) = match request.headers.get("range") {
            Some(range) => {
                // Parse range header
                match range.split("=").nth(1) {
                    Some(parts) => {
                        let parts: Vec<&str> = parts.trim().split('-').collect();
                        let start = match parts[0].parse::<u64>() {
                            Ok(start) => start,
                            Err(_) => 0,
                        };
                        let end = match parts.get(1) {
                            Some(end_str) if !end_str.is_empty() => match end_str.parse::<u64>() {
                                Ok(end) => end,
                                Err(_) => {
                                    let default_chunk_size: u64 = 1024 * 1024;
                                    let max_end = start + default_chunk_size - 1;
                                    std::cmp::min(max_end, file_size - 1)
                                }
                            },
                            _ => {
                                let default_chunk_size: u64 = 1024 * 1024;
                                let max_end = start + default_chunk_size - 1;
                                std::cmp::min(max_end, file_size - 1)
                            }
                        };
                        (start, end, true)
                    }
                    None => {
                        // First request - send first chunk to let RNTP know it supports ranges
                        println!("Initial request, sending first chunk");
                        let chunk_size = 1024 * 1024; // 1MB
                        (0, std::cmp::min(chunk_size - 1, file_size - 1), false)
                    }
                }
            }
            None => {
                // No range header - send full file or first chunk
                println!("No range header, sending full file");
                (0, file_size - 1, false)
            }
        };

        // Validate range
        if start >= file_size || end >= file_size || start > end {
            socket.write_all(BAD_REQUEST.as_bytes()).await?;
            return Ok(());
        }

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
        let mut buf = vec![0; content_length as usize];
        file.read_exact(&mut buf).await?;

        // Send appropriate header based on whether it's a range request
        let header = if is_range_request {
            format!(
                "HTTP/1.1 206 Partial Content\r\n\
            Content-Type: audio/mpeg\r\n\
            Content-Length: {}\r\n\
            Accept-Ranges: bytes\r\n\
            Access-Control-Allow-Origin: *\r\n\
            Content-Range: bytes {}-{}/{}\r\n\
            \r\n",
                content_length, start, end, file_size
            )
        } else {
            format!(
                "HTTP/1.1 200 OK\r\n\
            Content-Type: audio/mpeg\r\n\
            Content-Length: {}\r\n\
            Accept-Ranges: bytes\r\n\
            Access-Control-Allow-Origin: *\r\n\
            \r\n",
                content_length
            )
        };

        socket.write_all(header.as_bytes()).await?;
        socket.write_all(&buf).await?;
        Ok(())
    }
}
