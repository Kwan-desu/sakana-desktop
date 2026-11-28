use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tauri::State;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::db::Database;
use crate::models::{TranscribeResult, VoiceConfig};
use crate::DbState;

const MAX_AUDIO_BYTES: usize = 25 * 1024 * 1024;
const MAX_BASE64_CHARS: usize = ((MAX_AUDIO_BYTES + 2) / 3) * 4;

fn validate_whisper_url(raw: &str) -> Result<(), String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let parsed = reqwest::Url::parse(trimmed)
        .map_err(|e| format!("Invalid whisper_url: {e}"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("whisper_url must use http:// or https://".to_string());
    }
    if parsed.host_str().is_none() {
        return Err("whisper_url must include a host".to_string());
    }
    Ok(())
}

fn validate_whisper_cli_path(raw: &str) -> Result<(), String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let file_name = std::path::Path::new(trimmed)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let stem = file_name.strip_suffix(".exe").unwrap_or(&file_name);
    let allowed = matches!(
        stem,
        "whisper" | "whisper-cli" | "whisper-cpp" | "whisper.cpp" | "faster-whisper" | "whisper-ctranslate2"
    );
    if !allowed {
        return Err(
            "whisper_cli_path must point to an allowlisted whisper binary (whisper, whisper-cli, whisper-cpp, faster-whisper)"
                .to_string(),
        );
    }
    Ok(())
}

fn decode_base64(input: &str) -> Result<Vec<u8>, String> {
    // Fast pre-check before allocating to avoid huge memory spikes
    let non_ws_count = input.bytes().filter(|b| !b.is_ascii_whitespace()).count();
    if non_ws_count > MAX_BASE64_CHARS {
        return Err("Audio payload exceeds 25MB limit".to_string());
    }
    if non_ws_count % 4 != 0 {
        return Err("Invalid base64 audio payload".to_string());
    }
    let clean: Vec<u8> = input
        .bytes()
        .filter(|b| !b.is_ascii_whitespace())
        .collect();
    let mut out = Vec::with_capacity((clean.len() / 4) * 3);
    let val = |b: u8| -> Result<u8, String> {
        match b {
            b'A'..=b'Z' => Ok(b - b'A'),
            b'a'..=b'z' => Ok(b - b'a' + 26),
            b'0'..=b'9' => Ok(b - b'0' + 52),
            b'+' => Ok(62),
            b'/' => Ok(63),
            _ => Err("Invalid base64 character".to_string()),
        }
    };
    for chunk in clean.chunks_exact(4) {
        let a = val(chunk[0])?;
        let b = val(chunk[1])?;
        out.push((a << 2) | (b >> 4));
        if chunk[2] != b'=' {
            let c = val(chunk[2])?;
            out.push((b << 4) | (c >> 2));
            if chunk[3] != b'=' {
                let d = val(chunk[3])?;
                out.push((c << 6) | d);
            }
        }
    }
    Ok(out)
}

fn ext_for_mime(mime: &str) -> &'static str {
    let lower = mime.to_ascii_lowercase();
    if lower.contains("wav") {
        "wav"
    } else if lower.contains("ogg") {
        "ogg"
    } else if lower.contains("mp4") || lower.contains("m4a") {
        "m4a"
    } else if lower.contains("mp3") || lower.contains("mpeg") {
        "mp3"
    } else {
        "webm"
    }
}

fn short_lang_code(lang: &str) -> String {
    let trimmed = lang.trim();
    if trimmed.is_empty() {
        return "en".to_string();
    }
    trimmed
        .split(['-', '_'])
        .next()
        .unwrap_or("en")
        .to_ascii_lowercase()
}

pub async fn get_voice_config_inner(db: &Arc<Mutex<Database>>) -> Result<VoiceConfig, String> {
    let lock = db.lock().await;
    lock.get_voice_config().map_err(|e| e.to_string())
}

pub async fn update_voice_config_inner(
    db: &Arc<Mutex<Database>>,
    mut config: VoiceConfig,
    is_remote: bool,
) -> Result<VoiceConfig, String> {
    if !matches!(config.engine.as_str(), "auto" | "browser" | "whisper") {
        config.engine = "auto".to_string();
    }
    if config.language.trim().is_empty() {
        config.language = "en-US".to_string();
    }
    config.whisper_url = config.whisper_url.trim().to_string();
    config.whisper_cli_path = config.whisper_cli_path.trim().to_string();
    validate_whisper_url(&config.whisper_url)?;
    if is_remote {
        let existing = {
            let lock = db.lock().await;
            lock.get_voice_config().map_err(|e| e.to_string())?
        };
        if config.whisper_cli_path != existing.whisper_cli_path.trim() {
            return Err("Modifying whisper_cli_path is only permitted locally".to_string());
        }
    }
    validate_whisper_cli_path(&config.whisper_cli_path)?;
    config.silence_ms = config.silence_ms.clamp(400, 10_000);
    config.tts_rate = config.tts_rate.clamp(0.5, 2.0);
    let lock = db.lock().await;
    lock.save_voice_config(&config).map_err(|e| e.to_string())?;
    Ok(config)
}

async fn transcribe_with_cli(
    cli_path: &str,
    model: &str,
    language: &str,
    bytes: &[u8],
    ext: &str,
) -> Result<String, String> {
    validate_whisper_cli_path(cli_path)?;
    let tmp_path = std::env::temp_dir().join(format!("sakana-voice-{}.{}", Uuid::new_v4(), ext));
    std::fs::write(&tmp_path, bytes)
        .map_err(|e| format!("Failed to write temp audio file: {e}"))?;

    let mut cmd = crate::platform::tokio_command(cli_path);
    cmd.kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if !model.trim().is_empty() {
        cmd.args(["-m", model.trim()]);
    }
    let lang = short_lang_code(language);
    if !lang.is_empty() {
        cmd.args(["-l", &lang]);
    }
    cmd.args(["-nt", "-f", &tmp_path.to_string_lossy()]);
    let run_res = tokio::time::timeout(Duration::from_secs(60), cmd.output()).await;
    let _ = std::fs::remove_file(&tmp_path);

    let out = run_res
        .map_err(|_| "whisper-cli timed out after 60s".to_string())?
        .map_err(|e| format!("Failed to run whisper-cli ({cli_path}): {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!("whisper-cli failed: {}", stderr.trim()));
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Ok(text)
}

async fn transcribe_with_http(
    endpoint_url: &str,
    model: &str,
    language: &str,
    bytes: Vec<u8>,
    mime_type: &str,
) -> Result<String, String> {
    let url = if endpoint_url.trim().is_empty() {
        "https://api.openai.com/v1/audio/transcriptions".to_string()
    } else {
        validate_whisper_url(endpoint_url)?;
        let trimmed = endpoint_url.trim().trim_end_matches('/');
        if trimmed.ends_with("/audio/transcriptions") || trimmed.ends_with("/inference") {
            trimmed.to_string()
        } else {
            format!("{}/v1/audio/transcriptions", trimmed)
        }
    };

    let parsed_url = reqwest::Url::parse(&url).map_err(|e| format!("Invalid Whisper URL: {e}"))?;
    if !matches!(parsed_url.scheme(), "http" | "https") {
        return Err("Whisper URL must use http:// or https://".to_string());
    }
    let is_openai_official =
        parsed_url.scheme() == "https" && parsed_url.host_str() == Some("api.openai.com");

    let api_key = if is_openai_official {
        crate::platform::get_env_or_dotenv("WHISPER_API_KEY")
            .or_else(|| crate::platform::get_env_or_dotenv("OPENAI_API_KEY"))
    } else {
        crate::platform::get_env_or_dotenv("WHISPER_API_KEY")
    };

    if is_openai_official && api_key.is_none() {
        return Err(
            "No OPENAI_API_KEY configured in .env and no local Whisper URL set in Voice Settings."
                .to_string(),
        );
    }

    let ext = ext_for_mime(mime_type);
    let file_part = reqwest::multipart::Part::bytes(bytes)
        .file_name(format!("audio.{ext}"))
        .mime_str(if mime_type.trim().is_empty() {
            "audio/webm"
        } else {
            mime_type.trim()
        })
        .map_err(|e| format!("Invalid audio mime type: {e}"))?;

    let mut form = reqwest::multipart::Form::new()
        .part("file", file_part)
        .text(
            "model",
            if model.trim().is_empty() {
                "whisper-1".to_string()
            } else {
                model.trim().to_string()
            },
        )
        .text("response_format", "json");

    let lang = short_lang_code(language);
    if !lang.is_empty() {
        form = form.text("language", lang);
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;

    let mut req = client.post(&url).multipart(form);
    if let Some(key) = api_key {
        req = req.bearer_auth(key);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("Whisper request to {url} failed: {e}"))?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!("Whisper HTTP {}: {}", status.as_u16(), body.trim()));
    }
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
        if let Some(text) = json.get("text").and_then(|v| v.as_str()) {
            return Ok(text.trim().to_string());
        }
    }
    Ok(body.trim().to_string())
}

pub async fn transcribe_audio_inner(
    db: &Arc<Mutex<Database>>,
    audio_b64: String,
    mime_type: Option<String>,
    language: Option<String>,
) -> Result<TranscribeResult, String> {
    let started = Instant::now();
    let raw_b64 = audio_b64
        .split_once(",")
        .map(|(_, b)| b)
        .unwrap_or(&audio_b64);
    let bytes = decode_base64(raw_b64)?;
    if bytes.is_empty() {
        return Err("Empty audio payload".to_string());
    }
    if bytes.len() > MAX_AUDIO_BYTES {
        return Err("Audio payload exceeds 25MB limit".to_string());
    }

    let cfg = get_voice_config_inner(db).await?;
    let lang = language
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| cfg.language.clone());
    let mime = mime_type.unwrap_or_else(|| "audio/webm".to_string());
    let ext = ext_for_mime(&mime);

    let (text, engine) = if !cfg.whisper_cli_path.trim().is_empty() {
        let t = transcribe_with_cli(&cfg.whisper_cli_path, &cfg.whisper_model, &lang, &bytes, ext)
            .await?;
        (t, "whisper-cli".to_string())
    } else {
        let t = transcribe_with_http(&cfg.whisper_url, &cfg.whisper_model, &lang, bytes, &mime)
            .await?;
        (t, "whisper-http".to_string())
    };

    Ok(TranscribeResult {
        text,
        engine,
        duration_ms: started.elapsed().as_millis().max(1) as i64,
    })
}

#[tauri::command]
pub async fn get_voice_config(state: State<'_, DbState>) -> Result<VoiceConfig, String> {
    get_voice_config_inner(&state.0).await
}

#[tauri::command]
pub async fn update_voice_config(
    state: State<'_, DbState>,
    config: VoiceConfig,
) -> Result<VoiceConfig, String> {
    update_voice_config_inner(&state.0, config, false).await
}

#[tauri::command]
pub async fn transcribe_audio(
    state: State<'_, DbState>,
    audio_b64: String,
    mime_type: Option<String>,
    language: Option<String>,
) -> Result<TranscribeResult, String> {
    transcribe_audio_inner(&state.0, audio_b64, mime_type, language).await
}
