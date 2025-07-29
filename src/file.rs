use crate::constants::NOT_FOUND;
use crate::constants::OK_RESPONSE;
use crate::model::AddStream;
use crate::model::Track;
use crate::repo::Repository;
use chrono::Utc;
use once_cell::sync::Lazy;
use request_http_parser::parser::Request;
use serde_json::json;
use sqlx::Pool;
use sqlx::Postgres;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::fs::{self};
use tokio::io::AsyncWriteExt;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::model::TaskStatus;

static TASKS: Lazy<RwLock<HashMap<String, TaskStatus>>> = Lazy::new(|| RwLock::new(HashMap::new()));

pub struct File {}

impl File {
    pub async fn download_task(
        mut socket: tokio::net::TcpStream,
        request: Request,
        pool: Arc<Pool<Postgres>>,
    ) -> std::io::Result<()> {
        let body_str = match request.body {
            Some(body) => body,
            None => {
                let _ = socket
                    .write_all(format!("{}{}", NOT_FOUND, "404 Not Found").as_bytes())
                    .await;
                return Ok(());
            }
        };
        println!(" body str{}", body_str);
        let body: AddStream = match serde_json::from_str::<AddStream>(&body_str) {
            Ok(body) => body,
            Err(_) => {
                let _ = socket
                    .write_all(format!("{}{}", NOT_FOUND, "404 Not Found").as_bytes())
                    .await;
                return Ok(());
            }
        };
        println!(" body {:?}", body);

        let task_id = Self::spawn_download_task(body, pool).await;
        let payload = json!({
            "task_id": task_id,
        })
        .to_string();
        socket
            .write_all(format!("{}{}", OK_RESPONSE, payload).as_bytes())
            .await
            .expect("Failed to write");
        Ok(())
    }

    pub async fn get_task_status(
        mut socket: tokio::net::TcpStream,
        request: Request,
    ) -> std::io::Result<()> {
        let task_id = match &request.params {
            Some(params) => match params.get("task_id") {
                Some(song) => song,
                None => {
                    let _ = socket
                        .write_all(format!("{}{}", NOT_FOUND, "404 Not Found").as_bytes())
                        .await;
                    return Ok(());
                }
            },
            None => {
                let tasks = TASKS.read().await;
                let all_tasks: Vec<TaskStatus> = tasks.values().cloned().collect();
                let json = serde_json::to_string(&all_tasks).expect("error serializing all tasks");
                socket
                    .write_all(format!("{}{}", OK_RESPONSE, json).as_bytes())
                    .await
                    .expect("Failed to write");
                return Ok(());
            }
        };
        let tasks = TASKS.read().await;
        if let Some(status) = tasks.get(task_id) {
            let json: String = serde_json::to_string::<TaskStatus>(&status).expect("error serde");
            socket
                .write_all(format!("{}{}", OK_RESPONSE, json).as_bytes())
                .await
                .expect("Failed to write");
        } else {
            socket
                .write_all(format!("{}{}", NOT_FOUND, "404 Not Found").as_bytes())
                .await
                .expect("Failed to write");
        }

        Ok(())
    }
    pub async fn spawn_download_task(body: AddStream, pool: Arc<Pool<Postgres>>) -> String {
        print!("in");

        let task_id = Uuid::new_v4().to_string();
        let task_id_for_spawn = task_id.clone();

        let status = TaskStatus {
            task_id: task_id.clone(),
            title: body.title,
            status: "downloading".to_string(),
            progress: 0,
            log: vec![],
        };
        TASKS.write().await.insert(task_id.clone(), status.clone());

        tokio::spawn(async move {
            // Step 1: yt-dlp
            let mut yt_cmd = tokio::process::Command::new("yt-dlp")
                .arg("--extract-audio")
                .arg("--audio-format")
                .arg("mp3")
                .arg("-o")
                .arg("./mp3/%(title)s.%(ext)s")
                .arg(&body.youtube_url)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .expect("Failed to start yt-dlp");

            if let Some(stderr) = yt_cmd.stderr.take() {
                let reader = BufReader::new(stderr).lines();

                tokio::pin!(reader);

                while let Ok(Some(line)) = reader.next_line().await {
                    println!("line {}", line);
                    if line.contains("[download]") {
                        // Optional: parse %
                    }

                    let mut tasks = TASKS.write().await;
                    if let Some(t) = tasks.get_mut(&task_id) {
                        t.log.push(line.clone());
                    }
                }
            }

            let yt_status = yt_cmd.wait().await;
            print!("yt_status: {:?}", yt_status);

            if yt_status.is_err() || !yt_status.unwrap().success() {
                let mut tasks = TASKS.write().await;
                if let Some(t) = tasks.get_mut(&task_id) {
                    t.status = "failed".into();
                    t.log.push("yt-dlp failed".into());
                }
                return;
            }

            // get information from youtube
            let output = tokio::process::Command::new("yt-dlp")
                .arg("--print")
                .arg("%(title)s|||%(duration_string)s") // Use unique separator
                .arg(&body.youtube_url)
                .output()
                .await
                .expect("Failed to execute yt-dlp");

            let output_text = String::from_utf8_lossy(&output.stdout);
            let mut parts = output_text.trim().splitn(2, "|||");

            let title = parts
                .next()
                .ok_or("Missing title in output")
                .expect("")
                .trim()
                .to_string();

            let duration = parts
                .next()
                .ok_or("Missing duration in output")
                .expect("")
                .trim()
                .to_string();

            // Step 2: ffmpeg HLS
            let dir = format!("./hls/{}", &title);
            let dir_path = Path::new(&dir);
            fs::create_dir(dir_path)
                .await
                .expect("failed create hls title folder");
            let input_path = format!("./mp3/{}.mp3", title);
            let output_m3u8 = format!("{}/{}.m3u8", &dir, title);
            let output_ts_pattern = format!("{}/{}_%03d.ts", &dir, title);

            let mut ffmpeg_cmd = tokio::process::Command::new("ffmpeg")
                .args([
                    "-i",
                    &input_path,
                    "-c:a",
                    "aac",
                    "-b:a",
                    "128k",
                    "-ac",
                    "2",
                    "-ar",
                    "44100",
                    "-f",
                    "hls",
                    "-hls_time",
                    "10",
                    "-hls_playlist_type",
                    "vod",
                    "-hls_segment_filename",
                    &output_ts_pattern,
                    &output_m3u8,
                ])
                .stderr(std::process::Stdio::piped())
                .spawn()
                .expect("Failed to start ffmpeg");

            if let Some(stderr) = ffmpeg_cmd.stderr.take() {
                let reader = BufReader::new(stderr).lines();
                tokio::pin!(reader);
                while let Ok(Some(line)) = reader.next_line().await {
                    // Log progress
                    let mut tasks = TASKS.write().await;
                    if let Some(t) = tasks.get_mut(&task_id) {
                        t.log.push(line.clone());
                    }
                }
            }

            let ffmpeg_status = ffmpeg_cmd.wait().await;
            let mut tasks = TASKS.write().await;
            if let Some(t) = tasks.get_mut(&task_id) {
                if ffmpeg_status.is_ok() && ffmpeg_status.unwrap().success() {
                    // TODO SAVE DB
                    println!("save to db {} with duration {}", title, duration);
                    let new_track = Track {
                        title,
                        duration,
                        track_id: None,
                        created_at: Utc::now(),
                    };
                    let track_id = Repository::insert_track(&new_track, &pool)
                        .await
                        .expect("error insert db");
                    println!("inserted {}", track_id);
                    t.status = "done".into();
                    t.progress = 100;
                } else {
                    t.status = "failed".into();
                    t.log.push("ffmpeg failed".into());
                }
            }
        });

        task_id_for_spawn
    }
}
