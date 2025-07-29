use request_http_parser::parser::Request;
use std::io::SeekFrom;
use std::path::Path;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::net::TcpStream;

// TODO
// - if the title have "&" in there, the query got bug since we split the value with char "&",
// by now we avoid title with "&" char. maybe "=" made bug too(?)

pub struct HlsService {}

impl HlsService {
    pub async fn serve_hls_playlist(
        mut socket: TcpStream,
        request: Request,
    ) -> std::io::Result<()> {
        let song = match &request.params {
            Some(params) => match params.get("song") {
                Some(song) => song,
                None => {
                    let _ = socket
                        .write_all(b"HTTP/1.1 404 Not Found\r\n\r\n404 Not Found")
                        .await;
                    return Ok(());
                }
            },
            None => {
                let _ = socket
                    .write_all(b"HTTP/1.1 404 Not Found\r\n\r\n404 Not Found")
                    .await;
                return Ok(());
            }
        };
        let decoded_song = percent_encoding::percent_decode(song.as_bytes())
            .decode_utf8()
            .expect("error decode percent");

        println!("received {} dedode {}", song, decoded_song);

        let path = format!("./mp3/{}.mp3", decoded_song);
        let metadata = match tokio::fs::metadata(&path).await {
            Ok(metadata) => metadata,
            Err(_) => {
                let _ = socket
                    .write_all(b"HTTP/1.1 404 Not Found\r\n\r\n404 Not Found")
                    .await;
                return Ok(());
            }
        };

        let file_size = metadata.len();

        // Calculate segments (10 seconds each, approximate)
        let segment_duration = 10.0; // seconds
        let approx_bitrate = 128000; // 128 kbps
        let bytes_per_second = approx_bitrate / 8; // bits to bytes
        let segment_size = (bytes_per_second as f64 * segment_duration) as u64;

        let num_segments = (file_size + segment_size - 1) / segment_size;

        // Generate M3U8 playlist
        let mut playlist = String::new();
        playlist.push_str("#EXTM3U\n");
        playlist.push_str("#EXT-X-VERSION:3\n");
        playlist.push_str("#EXT-X-TARGETDURATION:11\n");
        playlist.push_str("#EXT-X-MEDIA-SEQUENCE:0\n");

        for i in 0..num_segments {
            let start = i * segment_size;
            let end = std::cmp::min(start + segment_size - 1, file_size - 1);
            let actual_duration = if i == num_segments - 1 {
                // Last segment might be shorter
                let remaining_bytes = file_size - start;
                (remaining_bytes as f64 / bytes_per_second as f64).min(segment_duration)
            } else {
                segment_duration
            };

            playlist.push_str(&format!("#EXTINF:{:.1},\n", actual_duration));
            println!("segment push");
            playlist.push_str(&format!(
                "/segment?song={}&start={}&end={}\n",
                song, start, end
            ));
        }

        playlist.push_str("#EXT-X-ENDLIST\n");

        // Send response
        let response = format!(
            "HTTP/1.1 200 OK\r\n\
        Content-Type: application/vnd.apple.mpegurl\r\n\
        Content-Length: {}\r\n\
        Access-Control-Allow-Origin: *\r\n\
        Cache-Control: no-cache\r\n\
        \r\n{}",
            playlist.len(),
            playlist
        );

        socket.write_all(response.as_bytes()).await?;
        Ok(())
    }

    // HLS segment handler
    pub async fn serve_hls_segment(mut socket: TcpStream, request: Request) -> std::io::Result<()> {
        let params = match &request.params {
            Some(params) => params,
            None => {
                let _ = socket.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await;
                return Ok(());
            }
        };

        let song_encoded = match params.get("song") {
            Some(song) => song,
            None => {
                let _ = socket.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await;
                return Ok(());
            }
        };

        let song = percent_encoding::percent_decode(song_encoded.as_bytes())
            .decode_utf8()
            .expect("error decode percent");

        println!("received {} dedode {}", song_encoded, song);

        let start: u64 = match params.get("start") {
            Some(start_str) => match start_str.parse() {
                Ok(start) => start,
                Err(_) => {
                    let _ = socket.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await;
                    return Ok(());
                }
            },
            None => {
                let _ = socket.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await;
                return Ok(());
            }
        };

        let end: u64 = match params.get("end") {
            Some(end_str) => match end_str.parse() {
                Ok(end) => end,
                Err(_) => {
                    let _ = socket.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await;
                    return Ok(());
                }
            },
            None => {
                let _ = socket.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await;
                return Ok(());
            }
        };

        let path = format!("./mp3/{}.mp3", song);
        let mut file = match File::open(&path).await {
            Ok(file) => file,
            Err(_) => {
                let _ = socket.write_all(b"HTTP/1.1 404 Not Found\r\n\r\n").await;
                return Ok(());
            }
        };

        let content_length = end - start + 1;

        // Seek to start position
        file.seek(SeekFrom::Start(start)).await?;

        // Read segment data
        let mut buffer = vec![0; content_length as usize];
        file.read_exact(&mut buffer).await?;

        // Send segment as MPEG-TS (for HLS compatibility)
        let response = format!(
            "HTTP/1.1 200 OK\r\n\
        Content-Type: video/mp2t\r\n\
        Content-Length: {}\r\n\
        Access-Control-Allow-Origin: *\r\n\
        Cache-Control: public, max-age=86400\r\n\
        \r\n",
            content_length
        );

        socket.write_all(response.as_bytes()).await?;
        socket.write_all(&buffer).await?;

        Ok(())
    }

    // Updated HLS playlist handler - serves pre-generated m3u8 files
    pub async fn serve_hls_playlist1(
        mut socket: TcpStream,
        request: Request,
    ) -> std::io::Result<()> {
        let song_enc = match &request.params {
            Some(params) => match params.get("song") {
                Some(song) => song,
                None => {
                    let _ = socket
                        .write_all(b"HTTP/1.1 404 Not Found\r\n\r\n404 Not Found")
                        .await;
                    return Ok(());
                }
            },
            None => {
                let _ = socket
                    .write_all(b"HTTP/1.1 404 Not Found\r\n\r\n404 Not Found")
                    .await;
                return Ok(());
            }
        };
        let song = percent_encoding::percent_decode(song_enc.as_bytes())
            .decode_utf8()
            .expect("error decode percent");

        println!("received {} dedode {}", song_enc, song);

        // Path to the pre-generated m3u8 file
        let playlist_path = format!("./hls/{}/{}.m3u8", song, song);

        // Check if the playlist file exists
        if !Path::new(&playlist_path).exists() {
            println!("gada ketemu {}", playlist_path);
            let _ = socket
                .write_all(b"HTTP/1.1 404 Not Found\r\n\r\n404 Not Found")
                .await;
            return Ok(());
        }

        // Read the existing m3u8 file
        let mut file = match File::open(&playlist_path).await {
            Ok(file) => file,
            Err(_) => {
                let _ = socket
                    .write_all(b"HTTP/1.1 404 Not Found\r\n\r\n404 Not Found")
                    .await;
                return Ok(());
            }
        };

        let mut playlist_content = String::new();
        if let Err(_) = file.read_to_string(&mut playlist_content).await {
            let _ = socket
                .write_all(b"HTTP/1.1 500 Internal Server Error\r\n\r\n500 Internal Server Error")
                .await;
            return Ok(());
        }

        // Modify the playlist to use our segment endpoint
        let modified_playlist = Self::modify_playlist_urls(&playlist_content, &song);
        println!("{:?}", modified_playlist);

        // Send response
        // let response = format!(
        //     "HTTP/1.1 200 OK\r\n\
        // Content-Type: application/vnd.apple.mpegurl\r\n\
        // Content-Length: {}\r\n\
        // Access-Control-Allow-Origin: *\r\n\
        // Cache-Control: no-cache\r\n\
        // \r\n{}",
        //     modified_playlist.len(),
        //     modified_playlist
        // );
        let response = format!(
            "HTTP/1.1 200 OK\r\n\
    Content-Type: application/vnd.apple.mpegurl\r\n\
    Content-Length: {}\r\n\
    Access-Control-Allow-Origin: *\r\n\
    Cache-Control: public, max-age=300\r\n\
    \r\n{}",
            modified_playlist.len(),
            modified_playlist
        );

        socket.write_all(response.as_bytes()).await?;
        Ok(())
    }

    // Helper function to modify playlist URLs to point to our segment handler
    fn modify_playlist_urls(playlist: &str, song: &str) -> String {
        let mut modified = String::new();

        for line in playlist.lines() {
            if line.ends_with(".ts") {
                // Extract the segment filename
                let segment_name = line.trim();
                // Replace with our segment endpoint
                modified.push_str(&format!("/segment?song={}&file={}\n", song, segment_name));
            } else {
                modified.push_str(line);
                modified.push('\n');
            }
        }

        modified
    }

    // Updated HLS segment handler - serves pre-generated .ts files
    pub async fn serve_hls_segment1(
        mut socket: TcpStream,
        request: Request,
    ) -> std::io::Result<()> {
        let params = match &request.params {
            Some(params) => params,
            None => {
                let _ = socket.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await;
                return Ok(());
            }
        };

        let song_enc = match params.get("song") {
            Some(song) => song,
            None => {
                let _ = socket.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await;
                return Ok(());
            }
        };

        let song = percent_encoding::percent_decode(song_enc.as_bytes())
            .decode_utf8()
            .expect("error decode percent");

        println!("received {} dedode {}", song_enc, song);

        let segment_file_enc = match params.get("file") {
            Some(file) => file,
            None => {
                let _ = socket.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await;
                return Ok(());
            }
        };

        let segment_file = percent_encoding::percent_decode(segment_file_enc.as_bytes())
            .decode_utf8()
            .expect("error decode percent");

        println!("received {} dedode {}", segment_file_enc, segment_file);

        // Path to the pre-generated .ts file
        let segment_path = format!("./hls/{}/{}", song, segment_file);

        // Check if the segment file exists
        if !Path::new(&segment_path).exists() {
            let _ = socket.write_all(b"HTTP/1.1 404 Not Found\r\n\r\n").await;
            return Ok(());
        }

        // Read the .ts file
        let mut file = match File::open(&segment_path).await {
            Ok(file) => file,
            Err(_) => {
                let _ = socket.write_all(b"HTTP/1.1 404 Not Found\r\n\r\n").await;
                return Ok(());
            }
        };

        let mut buffer = Vec::new();
        if let Err(_) = file.read_to_end(&mut buffer).await {
            let _ = socket
                .write_all(b"HTTP/1.1 500 Internal Server Error\r\n\r\n")
                .await;
            return Ok(());
        }

        // Send the pre-generated .ts segment
        let response = format!(
            "HTTP/1.1 200 OK\r\n\
        Content-Type: video/mp2t\r\n\
        Content-Length: {}\r\n\
        Access-Control-Allow-Origin: *\r\n\
        Cache-Control: public, max-age=86400\r\n\
        \r\n",
            buffer.len()
        );

        socket.write_all(response.as_bytes()).await?;
        socket.write_all(&buffer).await?;

        Ok(())
    }
}
