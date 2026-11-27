use std::collections::HashMap;
use std::convert::Infallible;
use std::net::UdpSocket;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path as AxumPath, Query, State as AxumState};
use axum::http::{header, HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::StreamExt;
use qrcode::QrCode;
use serde_json::{json, Value};
use sysinfo::System;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::{oneshot, Mutex};
use tokio_stream::wrappers::BroadcastStream;
use tower_http::cors::{AllowOrigin, CorsLayer};
use uuid::Uuid;

use crate::auth::AuthState;
use crate::db::Database;
use crate::harness::HarnessState;
use crate::models::{
    CreateOfficeInput, CreateProjectInput, CreateSessionInput, HireStaffInput, HostInfo,
    RemoteHost, RemoteStatus, UpdateSessionInput, UpdateStaffInput, UpsertRemoteHostInput,
};
use crate::pty::PtyState;
use crate::DbState;

const DEFAULT_REMOTE_PORT: u16 = 4317;
const REMOTE_AUTH_TOKEN_KEY: &str = "remote_auth_token";

pub struct RemoteState {
    pub running: Arc<Mutex<bool>>,
    pub port: Arc<Mutex<u16>>,
    pub bind_addr: Arc<Mutex<String>>,
    pub token: Arc<Mutex<String>>,
    pub host_id: String,
    pub last_error: Arc<Mutex<Option<String>>>,
    pub shutdown_tx: Arc<Mutex<Option<oneshot::Sender<()>>>>,
    pub event_client_stop: Arc<Mutex<Option<oneshot::Sender<()>>>>,
    pub http_client: reqwest::Client,
}

impl Default for RemoteState {
    fn default() -> Self {
        let initial_token = std::env::var("SAKANA_TOKEN")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| Uuid::new_v4().to_string().replace('-', ""));
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .unwrap_or_default();
        Self {
            running: Arc::new(Mutex::new(false)),
            port: Arc::new(Mutex::new(DEFAULT_REMOTE_PORT)),
            bind_addr: Arc::new(Mutex::new("127.0.0.1".to_string())),
            token: Arc::new(Mutex::new(initial_token)),
            host_id: format!("host-{}", &Uuid::new_v4().simple().to_string()[..8]),
            last_error: Arc::new(Mutex::new(None)),
            shutdown_tx: Arc::new(Mutex::new(None)),
            event_client_stop: Arc::new(Mutex::new(None)),
            http_client,
        }
    }
}

#[derive(Clone)]
pub struct ServerContext {
    pub db: Arc<Mutex<Database>>,
    pub harness_active: Arc<Mutex<HashMap<String, oneshot::Sender<()>>>>,
    pub office_active: Arc<Mutex<HashMap<String, tokio::sync::watch::Sender<bool>>>>,
    pub pty_state: Arc<PtyState>,
    pub auth_state: Arc<AuthState>,
    pub discord_state: Arc<crate::discord::DiscordState>,
    pub app: Option<AppHandle>,
    pub token: String,
    pub port: u16,
    pub host_id: String,
}

pub fn detect_lan_ip() -> String {
    if let Ok(sock) = UdpSocket::bind("0.0.0.0:0") {
        if sock.connect("8.8.8.8:80").is_ok() {
            if let Ok(addr) = sock.local_addr() {
                return addr.ip().to_string();
            }
        }
    }
    "127.0.0.1".to_string()
}

fn is_tailscale_ipv4(ip: &str) -> bool {
    let Ok(addr) = ip.trim().parse::<std::net::Ipv4Addr>() else {
        return false;
    };
    let octets = addr.octets();
    // Tailscale uses CGNAT range 100.64.0.0/10 (100.64.0.0 - 100.127.255.255)
    octets[0] == 100 && (64..=127).contains(&octets[1])
}

fn run_tailscale_cli(args: &[&str]) -> Option<String> {
    let mut candidates: Vec<&str> = vec!["tailscale"];
    #[cfg(target_os = "windows")]
    {
        candidates.push(r"C:\Program Files\Tailscale\tailscale.exe");
        candidates.push(r"C:\Program Files (x86)\Tailscale\tailscale.exe");
    }
    #[cfg(target_os = "macos")]
    {
        candidates.push("/Applications/Tailscale.app/Contents/MacOS/Tailscale");
        candidates.push("/usr/local/bin/tailscale");
    }
    for bin in candidates {
        let mut cmd = std::process::Command::new(bin);
        cmd.args(args);
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        if let Ok(out) = cmd.output() {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }
    }
    None
}

/// Detect Tailscale IPv4 (`100.64.0.0/10`) and MagicDNS hostname (`*.ts.net`).
pub fn detect_tailscale_info() -> (Option<String>, Option<String>) {
    let mut ts_ip = run_tailscale_cli(&["ip", "-4"])
        .and_then(|s| s.lines().next().map(|l| l.trim().to_string()))
        .filter(|ip| is_tailscale_ipv4(ip));

    // Fallback without CLI: probe route to Tailscale MagicDNS resolver 100.100.100.100
    if ts_ip.is_none() {
        if let Ok(sock) = UdpSocket::bind("0.0.0.0:0") {
            if sock.connect("100.100.100.100:53").is_ok() {
                if let Ok(addr) = sock.local_addr() {
                    let candidate = addr.ip().to_string();
                    if is_tailscale_ipv4(&candidate) {
                        ts_ip = Some(candidate);
                    }
                }
            }
        }
    }

    let ts_dns = run_tailscale_cli(&["status", "--json"]).and_then(|raw| {
        let v: Value = serde_json::from_str(&raw).ok()?;
        let dns = v
            .get("Self")
            .and_then(|s| s.get("DNSName"))
            .and_then(|d| d.as_str())?
            .trim()
            .trim_end_matches('.')
            .to_string();
        if dns.is_empty() { None } else { Some(dns) }
    });

    (ts_ip, ts_dns)
}

fn detect_hostname() -> String {
    System::host_name().unwrap_or_else(|| "sakana-host".to_string())
}

fn render_qr_svg(content: &str) -> String {
    match QrCode::new(content.as_bytes()) {
        Ok(code) => code
            .render::<qrcode::render::svg::Color<'_>>()
            .min_dimensions(180, 180)
            .dark_color(qrcode::render::svg::Color("#111111"))
            .light_color(qrcode::render::svg::Color("#ffffff"))
            .build(),
        Err(_) => String::new(),
    }
}

pub fn build_pairing_link(ip: &str, port: u16, token: &str) -> String {
    let host = detect_hostname();
    format!("sakana://{}:{}?token={}&name={}", ip, port, token, host)
}

async fn build_remote_status(state: &RemoteState) -> RemoteStatus {
    let running = *state.running.lock().await;
    let port = *state.port.lock().await;
    let bind_addr = state.bind_addr.lock().await.clone();
    let token = state.token.lock().await.clone();
    let last_error = state.last_error.lock().await.clone();
    let lan_ip = detect_lan_ip();
    let (tailscale_ip, tailscale_dns) = tokio::task::spawn_blocking(detect_tailscale_info)
        .await
        .unwrap_or((None, None));
    let tailscale_magic_link = tailscale_ip
        .as_deref()
        .or(tailscale_dns.as_deref())
        .map(|ts_host| build_pairing_link(ts_host, port, &token));
    let host_ip = if bind_addr == "0.0.0.0" {
        tailscale_ip.clone().unwrap_or_else(|| lan_ip.clone())
    } else {
        "127.0.0.1".to_string()
    };
    let magic_link = build_pairing_link(&host_ip, port, &token);
    let qr_svg = render_qr_svg(&magic_link);

    RemoteStatus {
        running,
        port,
        bind_addr,
        token,
        magic_link,
        qr_svg,
        last_error,
        lan_ip: Some(lan_ip),
        tailscale_ip,
        tailscale_dns,
        tailscale_magic_link,
    }
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (&x, &y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

pub fn load_or_init_remote_token(db: &Database, fallback: &str) -> String {
    if let Ok(env_tok) = std::env::var("SAKANA_TOKEN") {
        let trimmed = env_tok.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    if let Ok(Some(saved)) = db.get_setting(REMOTE_AUTH_TOKEN_KEY) {
        if !saved.trim().is_empty() {
            return saved;
        }
    }
    let _ = db.set_setting(REMOTE_AUTH_TOKEN_KEY, fallback);
    fallback.to_string()
}

fn is_authorized(
    headers: &HeaderMap,
    query: &HashMap<String, String>,
    expected: &str,
    allow_query_token: bool,
) -> bool {
    if let Some(auth_val) = headers.get("authorization").and_then(|v| v.to_str().ok()) {
        if let Some(bearer) = auth_val
            .strip_prefix("Bearer ")
            .or_else(|| auth_val.strip_prefix("bearer "))
        {
            if constant_time_eq(bearer.trim(), expected) {
                return true;
            }
        }
    }
    if let Some(header_tok) = headers.get("x-sakana-token").and_then(|v| v.to_str().ok()) {
        if constant_time_eq(header_tok.trim(), expected) {
            return true;
        }
    }
    if allow_query_token {
        if let Some(query_tok) = query.get("token") {
            if constant_time_eq(query_tok, expected) {
                return true;
            }
        }
    }
    false
}

async fn api_health(AxumState(ctx): AxumState<ServerContext>) -> impl IntoResponse {
    Json(json!({
        "ok": true,
        "service": "sakana-remote",
        "protocol": "sakana-rpc-v2",
        "port": ctx.port,
        "os": std::env::consts::OS,
        "hostname": detect_hostname(),
    }))
}

async fn api_host_info(
    AxumState(ctx): AxumState<ServerContext>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    if !is_authorized(&headers, &query, &ctx.token, false) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Invalid or missing Sakana token" })),
        )
            .into_response();
    }
    let info = HostInfo {
        host_id: ctx.host_id.clone(),
        hostname: detect_hostname(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        port: ctx.port,
    };
    (StatusCode::OK, Json(info)).into_response()
}

async fn api_bootstrap(
    AxumState(ctx): AxumState<ServerContext>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    if !is_authorized(&headers, &query, &ctx.token, false) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Invalid or missing Sakana token" })),
        )
            .into_response();
    }
    let db_lock = ctx.db.lock().await;
    match db_lock.get_bootstrap_state() {
        Ok(state) => (StatusCode::OK, Json(state)).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

async fn api_events(
    AxumState(ctx): AxumState<ServerContext>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    if !is_authorized(&headers, &query, &ctx.token, true) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Invalid or missing Sakana token" })),
        )
            .into_response();
    }
    let rx = crate::platform::subscribe_events();
    let stream = BroadcastStream::new(rx).filter_map(|item| async move {
        match item {
            Ok(ev) => {
                let data = serde_json::to_string(&ev).ok()?;
                Some(Ok::<Event, Infallible>(Event::default().event("sakana").data(data)))
            }
            Err(_) => None,
        }
    });
    Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)).text("ping"))
        .into_response()
}

fn get_str(args: &Value, key: &str) -> Result<String, String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("Missing string argument '{}'", key))
}

fn get_opt_str(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}

async fn dispatch_rpc(ctx: &ServerContext, cmd: &str, args: Value) -> Result<Value, String> {
    match cmd {
        "get_bootstrap_state" => {
            let db = ctx.db.lock().await;
            let state = db.get_bootstrap_state().map_err(|e| e.to_string())?;
            serde_json::to_value(state).map_err(|e| e.to_string())
        }
        "list_projects" => {
            let db = ctx.db.lock().await;
            let items = db.list_projects().map_err(|e| e.to_string())?;
            serde_json::to_value(items).map_err(|e| e.to_string())
        }
        "create_project" => {
            let input_val = args.get("input").cloned().unwrap_or(args);
            let input: CreateProjectInput =
                serde_json::from_value(input_val).map_err(|e| e.to_string())?;
            let db = ctx.db.lock().await;
            let proj = db.create_project(input).map_err(|e| e.to_string())?;
            serde_json::to_value(proj).map_err(|e| e.to_string())
        }
        "rename_project" => {
            let id = get_str(&args, "id")?;
            let name = get_str(&args, "name")?;
            let db = ctx.db.lock().await;
            let ok = db.rename_project(&id, &name).map_err(|e| e.to_string())?;
            Ok(json!(ok))
        }
        "reorder_projects" => {
            let ordered_ids: Vec<String> = serde_json::from_value(
                args.get("orderedIds")
                    .or_else(|| args.get("ordered_ids"))
                    .cloned()
                    .unwrap_or_else(|| json!([])),
            )
            .map_err(|e| e.to_string())?;
            let db = ctx.db.lock().await;
            db.reorder_projects(&ordered_ids).map_err(|e| e.to_string())?;
            Ok(json!(true))
        }
        "reorder_sessions" => {
            let ordered_ids: Vec<String> = serde_json::from_value(
                args.get("orderedIds")
                    .or_else(|| args.get("ordered_ids"))
                    .cloned()
                    .unwrap_or_else(|| json!([])),
            )
            .map_err(|e| e.to_string())?;
            let db = ctx.db.lock().await;
            db.reorder_sessions(&ordered_ids).map_err(|e| e.to_string())?;
            Ok(json!(true))
        }
        "set_project_discord_channel" => {
            let id = get_str(&args, "id")?;
            let channel = get_opt_str(&args, "discordChannel")
                .or_else(|| get_opt_str(&args, "discord_channel"));
            let db = ctx.db.lock().await;
            let ok = db
                .set_project_discord_channel(&id, channel.as_deref())
                .map_err(|e| e.to_string())?;
            Ok(json!(ok))
        }
        "set_session_discord_channel" => {
            let id = get_str(&args, "id")?;
            let channel = get_opt_str(&args, "discordChannel")
                .or_else(|| get_opt_str(&args, "discord_channel"));
            let db = ctx.db.lock().await;
            let ok = db
                .set_session_discord_channel(&id, channel.as_deref())
                .map_err(|e| e.to_string())?;
            Ok(json!(ok))
        }
        "delete_project" => {
            let id = get_str(&args, "id")?;
            let db = ctx.db.lock().await;
            let ok = db.delete_project(&id).map_err(|e| e.to_string())?;
            Ok(json!(ok))
        }
        "list_sessions" => {
            let pid = get_opt_str(&args, "projectId").or_else(|| get_opt_str(&args, "project_id"));
            let db = ctx.db.lock().await;
            let items = db.list_sessions(pid.as_deref()).map_err(|e| e.to_string())?;
            serde_json::to_value(items).map_err(|e| e.to_string())
        }
        "create_session" => {
            let input_val = args.get("input").cloned().unwrap_or(args);
            let input: CreateSessionInput =
                serde_json::from_value(input_val).map_err(|e| e.to_string())?;
            let db = ctx.db.lock().await;
            let sess = db.create_session(input).map_err(|e| e.to_string())?;
            serde_json::to_value(sess).map_err(|e| e.to_string())
        }
        "update_session" => {
            let input_val = args.get("input").cloned().unwrap_or(args);
            let input: UpdateSessionInput =
                serde_json::from_value(input_val).map_err(|e| e.to_string())?;
            let db = ctx.db.lock().await;
            let sess = db.update_session(input).map_err(|e| e.to_string())?;
            serde_json::to_value(sess).map_err(|e| e.to_string())
        }
        "delete_session" => {
            let id = get_str(&args, "id")?;
            let db = ctx.db.lock().await;
            let ok = db.delete_session(&id).map_err(|e| e.to_string())?;
            Ok(json!(ok))
        }
        "fork_session" => {
            let source_id = get_str(&args, "sourceId").or_else(|_| get_str(&args, "source_id"))?;
            let title = get_str(&args, "title")?;
            let branch = get_str(&args, "branch")?;
            let up_to_index = args
                .get("upToIndex")
                .or_else(|| args.get("up_to_index"))
                .and_then(|v| v.as_u64())
                .ok_or_else(|| "Missing 'upToIndex' parameter".to_string())?
                as usize;
            let db = ctx.db.lock().await;
            let sess = db
                .fork_session(&source_id, &title, &branch, up_to_index)
                .map_err(|e| e.to_string())?;
            serde_json::to_value(sess).map_err(|e| e.to_string())
        }
        "set_theme" => {
            let theme = get_str(&args, "theme")?;
            let db = ctx.db.lock().await;
            db.set_theme(&theme).map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        "list_directories" => {
            let path = get_opt_str(&args, "path");
            let wsl_distro = get_opt_str(&args, "wslDistro").or_else(|| get_opt_str(&args, "wsl_distro"));
            let listing = tokio::task::spawn_blocking(move || {
                crate::platform::list_directories_impl(path.as_deref(), wsl_distro.as_deref())
            })
            .await
            .map_err(|e| e.to_string())??;
            serde_json::to_value(listing).map_err(|e| e.to_string())
        }
        "list_wsl_distros" => {
            let distros = tokio::task::spawn_blocking(crate::platform::detect_wsl_distros)
                .await
                .map_err(|e| e.to_string())?;
            serde_json::to_value(distros).map_err(|e| e.to_string())
        }
        "send_turn" => {
            let session_id = get_str(&args, "sessionId").or_else(|_| get_str(&args, "session_id"))?;
            let prompt = get_str(&args, "prompt")?;
            let attachments = args
                .get("attachments")
                .and_then(|v| v.as_array())
                .cloned();
            let peer_session_id = get_opt_str(&args, "peerSessionId")
                .or_else(|| get_opt_str(&args, "peer_session_id"));
            let msg_id = crate::harness::send_turn_with_options(
                ctx.app.clone(),
                Arc::clone(&ctx.db),
                Arc::clone(&ctx.harness_active),
                session_id,
                prompt,
                attachments,
                crate::harness::TurnRunOptions {
                    peer_session_id,
                    ..Default::default()
                },
            )
            .await?;
            Ok(json!(msg_id))
        }
        "sync_get_sessions" => {
            let db = ctx.db.lock().await;
            let payload = crate::sync::export_sessions_payload(&db)?;
            serde_json::to_value(payload).map_err(|e| e.to_string())
        }
        "sync_push_sessions" => {
            let payload: crate::sync::SyncSessionsPayload =
                serde_json::from_value(args).map_err(|e| e.to_string())?;
            let (touched, added) = {
                let db = ctx.db.lock().await;
                crate::sync::merge_sessions_payload(&db, &payload)?
            };
            if added > 0 {
                crate::platform::emit_app_and_remote(
                    ctx.app.as_ref(),
                    "sakana:sync-updated",
                    &json!({ "newMessages": added, "filesPulled": 0, "filesPushed": 0 }),
                );
            }
            Ok(json!({ "sessionsTouched": touched, "newMessages": added }))
        }
        "sync_list_file_manifest" => {
            let cwd = get_str(&args, "cwd")?;
            let wsl_distro =
                get_opt_str(&args, "wslDistro").or_else(|| get_opt_str(&args, "wsl_distro"));
            let entries = tokio::task::spawn_blocking(move || {
                crate::sync::list_project_file_manifest(&cwd, wsl_distro.as_deref())
            })
            .await
            .map_err(|e| e.to_string())??;
            serde_json::to_value(entries).map_err(|e| e.to_string())
        }
        "sync_read_file_b64" => {
            let cwd = get_str(&args, "cwd")?;
            let rel_path = get_str(&args, "relPath").or_else(|_| get_str(&args, "rel_path"))?;
            let wsl_distro =
                get_opt_str(&args, "wslDistro").or_else(|| get_opt_str(&args, "wsl_distro"));
            tokio::task::spawn_blocking(move || {
                crate::sync::read_project_file_b64(&cwd, &rel_path, wsl_distro.as_deref())
            })
            .await
            .map_err(|e| e.to_string())?
        }
        "sync_write_file_b64" => {
            let cwd = get_str(&args, "cwd")?;
            let rel_path = get_str(&args, "relPath").or_else(|_| get_str(&args, "rel_path"))?;
            let content_b64 =
                get_str(&args, "contentB64").or_else(|_| get_str(&args, "content_b64"))?;
            let wsl_distro =
                get_opt_str(&args, "wslDistro").or_else(|| get_opt_str(&args, "wsl_distro"));
            let ok = tokio::task::spawn_blocking(move || {
                crate::sync::write_project_file_b64(
                    &cwd,
                    &rel_path,
                    &content_b64,
                    wsl_distro.as_deref(),
                )
            })
            .await
            .map_err(|e| e.to_string())??;
            Ok(json!(ok))
        }
        "cancel_turn" => {
            let session_id = get_str(&args, "sessionId").or_else(|_| get_str(&args, "session_id"))?;
            let mut active = ctx.harness_active.lock().await;
            if let Some(tx) = active.remove(&session_id) {
                let _ = tx.send(());
            }
            Ok(Value::Null)
        }
        "list_harness_models" => {
            let refresh = args.get("refresh").and_then(|v| v.as_bool()).unwrap_or(false);
            let models = crate::harness::list_harness_models_with_db(&ctx.db, refresh).await?;
            serde_json::to_value(models).map_err(|e| e.to_string())
        }
        "list_skills" => {
            let cwd = get_opt_str(&args, "projectCwd").or_else(|| get_opt_str(&args, "project_cwd"));
            let skills = crate::harness::list_skills(cwd).await?;
            serde_json::to_value(skills).map_err(|e| e.to_string())
        }
        "list_harness_accounts" => {
            let harness = get_opt_str(&args, "harness");
            let rows = {
                let db = ctx.db.lock().await;
                db.list_harness_accounts(harness.as_deref())
                    .map_err(|e| e.to_string())?
            };
            let outputs = ctx.auth_state.last_outputs.lock().await;
            let items: Vec<_> = rows
                .into_iter()
                .map(|r| {
                    let authed = crate::auth::is_account_authenticated(&r);
                    let cli_bin = crate::harness::resolve_harness_binary(&r.harness)
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default();
                    let last_out = outputs.get(&r.id).cloned().unwrap_or_default();
                    crate::models::HarnessAuthAccount {
                        id: r.id,
                        harness: r.harness,
                        label: r.label,
                        email: r.email,
                        profile_dir: r.profile_dir,
                        auth_type: r.auth_type,
                        is_default: r.is_default,
                        authenticated: authed,
                        status: if authed { "authenticated".to_string() } else { r.status },
                        cli_installed: !cli_bin.is_empty(),
                        cli_bin,
                        last_login_output: last_out,
                    }
                })
                .collect();
            serde_json::to_value(items).map_err(|e| e.to_string())
        }
        "get_git_diffs" => {
            let wsl_distro = get_opt_str(&args, "wslDistro").or_else(|| get_opt_str(&args, "wsl_distro"));
            let cwd = get_str(&args, "cwd")?;
            let turn_id = get_opt_str(&args, "turnId").or_else(|| get_opt_str(&args, "turn_id"));
            let res = crate::git::get_git_diffs_inner(&ctx.db, cwd, turn_id, wsl_distro).await?;
            serde_json::to_value(res).map_err(|e| e.to_string())
        }
        "list_branches" => {
            let wsl_distro = get_opt_str(&args, "wslDistro").or_else(|| get_opt_str(&args, "wsl_distro"));
            let cwd = get_str(&args, "cwd")?;
            let res = crate::git::list_branches_inner(&ctx.db, cwd, wsl_distro).await?;
            serde_json::to_value(res).map_err(|e| e.to_string())
        }
        "checkout_branch" => {
            let wsl_distro = get_opt_str(&args, "wslDistro").or_else(|| get_opt_str(&args, "wsl_distro"));
            let cwd = get_str(&args, "cwd")?;
            let branch = get_str(&args, "branch")?;
            crate::git::checkout_branch_inner(&ctx.db, cwd, branch, wsl_distro).await?;
            Ok(Value::Null)
        }
        "git_pull" => {
            let wsl_distro = get_opt_str(&args, "wslDistro").or_else(|| get_opt_str(&args, "wsl_distro"));
            let cwd = get_str(&args, "cwd")?;
            let out = crate::git::git_pull_inner(&ctx.db, cwd, wsl_distro).await?;
            Ok(json!(out))
        }
        "git_push" => {
            let wsl_distro = get_opt_str(&args, "wslDistro").or_else(|| get_opt_str(&args, "wsl_distro"));
            let cwd = get_str(&args, "cwd")?;
            let out = crate::git::git_push_inner(&ctx.db, cwd, wsl_distro).await?;
            Ok(json!(out))
        }
        "create_worktree" => {
            let session_id = get_str(&args, "sessionId").or_else(|_| get_str(&args, "session_id"))?;
            let project_cwd = get_str(&args, "projectCwd").or_else(|_| get_str(&args, "project_cwd"))?;
            let branch = get_str(&args, "branch")?;
            let shared_deps: Vec<String> = args
                .get("sharedDeps")
                .or_else(|| args.get("shared_deps"))
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let wt = crate::git::create_worktree_inner(&ctx.db, session_id, project_cwd, branch, shared_deps).await?;
            Ok(json!(wt))
        }
        "remove_worktree" => {
            let session_id = get_str(&args, "sessionId").or_else(|_| get_str(&args, "session_id"))?;
            let project_cwd = get_str(&args, "projectCwd").or_else(|_| get_str(&args, "project_cwd"))?;
            crate::git::remove_worktree_inner(&ctx.db, session_id, project_cwd).await?;
            Ok(Value::Null)
        }
        "create_pr" => {
            let wsl_distro = get_opt_str(&args, "wslDistro").or_else(|| get_opt_str(&args, "wsl_distro"));
            let cwd = get_str(&args, "cwd")?;
            let title = get_str(&args, "title")?;
            let body = get_opt_str(&args, "body").unwrap_or_default();
            let base_branch = get_str(&args, "baseBranch").or_else(|_| get_str(&args, "base_branch"))?;
            let url = crate::git::create_pr_inner(&ctx.db, cwd, title, body, base_branch, wsl_distro).await?;
            Ok(json!(url))
        }
        "stage_hunk" => {
            let wsl_distro = get_opt_str(&args, "wslDistro").or_else(|| get_opt_str(&args, "wsl_distro"));
            let cwd = get_str(&args, "cwd")?;
            let file_path = get_str(&args, "filePath").or_else(|_| get_str(&args, "file_path"))?;
            let hunk_header = get_str(&args, "hunkHeader").or_else(|_| get_str(&args, "hunk_header"))?;
            let stage = args.get("stage").and_then(|v| v.as_bool()).unwrap_or(true);
            crate::git::stage_hunk_inner(&ctx.db, cwd, file_path, hunk_header, stage, wsl_distro).await?;
            Ok(Value::Null)
        }
        "discard_hunk" => {
            let wsl_distro = get_opt_str(&args, "wslDistro").or_else(|| get_opt_str(&args, "wsl_distro"));
            let cwd = get_str(&args, "cwd")?;
            let file_path = get_str(&args, "filePath").or_else(|_| get_str(&args, "file_path"))?;
            let hunk_header = get_str(&args, "hunkHeader").or_else(|_| get_str(&args, "hunk_header"))?;
            crate::git::discard_hunk_inner(&ctx.db, cwd, file_path, hunk_header, wsl_distro).await?;
            Ok(Value::Null)
        }
        "draft_commit_message" => {
            let wsl_distro = get_opt_str(&args, "wslDistro").or_else(|| get_opt_str(&args, "wsl_distro"));
            let cwd = get_str(&args, "cwd")?;
            let msg = crate::git::draft_commit_message_inner(&ctx.db, cwd, wsl_distro).await?;
            Ok(json!(msg))
        }
        "git_commit" => {
            let wsl_distro = get_opt_str(&args, "wslDistro").or_else(|| get_opt_str(&args, "wsl_distro"));
            let cwd = get_str(&args, "cwd")?;
            let message = get_str(&args, "message")?;
            let out = crate::git::git_commit_inner(&ctx.db, cwd, message, wsl_distro).await?;
            Ok(json!(out))
        }
        "list_workspace_files" => {
            let wsl_distro = get_opt_str(&args, "wslDistro").or_else(|| get_opt_str(&args, "wsl_distro"));
            let cwd = get_str(&args, "cwd")?;
            let files = crate::git::list_workspace_files_inner(&ctx.db, cwd, wsl_distro).await?;
            serde_json::to_value(files).map_err(|e| e.to_string())
        }
        "read_workspace_file" => {
            let wsl_distro = get_opt_str(&args, "wslDistro").or_else(|| get_opt_str(&args, "wsl_distro"));
            let cwd = get_str(&args, "cwd")?;
            let rel_path = get_str(&args, "relPath").or_else(|_| get_str(&args, "rel_path"))?;
            let file = crate::git::read_workspace_file_inner(&ctx.db, cwd, rel_path, wsl_distro).await?;
            serde_json::to_value(file).map_err(|e| e.to_string())
        }
        "write_workspace_file" => {
            let wsl_distro = get_opt_str(&args, "wslDistro").or_else(|| get_opt_str(&args, "wsl_distro"));
            let cwd = get_str(&args, "cwd")?;
            let rel_path = get_str(&args, "relPath").or_else(|_| get_str(&args, "rel_path"))?;
            let content = get_str(&args, "content")?;
            crate::git::write_workspace_file_inner(&ctx.db, cwd, rel_path, content, wsl_distro).await?;
            Ok(Value::Null)
        }
        "pty_spawn" => {
            let session_id = get_str(&args, "sessionId").or_else(|_| get_str(&args, "session_id"))?;
            let cwd = get_str(&args, "cwd")?;
            let cols = args.get("cols").and_then(|v| v.as_u64()).unwrap_or(100) as u16;
            let rows = args.get("rows").and_then(|v| v.as_u64()).unwrap_or(28) as u16;
            let wsl_distro = get_opt_str(&args, "wslDistro").or_else(|| get_opt_str(&args, "wsl_distro"));
            crate::pty::pty_spawn_inner(
                ctx.app.clone(),
                &ctx.pty_state,
                session_id,
                cwd,
                cols,
                rows,
                wsl_distro,
            )
            .await?;
            Ok(Value::Null)
        }
        "pty_write" => {
            let session_id = get_str(&args, "sessionId").or_else(|_| get_str(&args, "session_id"))?;
            let data = get_str(&args, "data")?;
            let writer = {
                let map = ctx.pty_state.sessions.lock().await;
                let sess = map
                    .get(&session_id)
                    .ok_or_else(|| format!("No active PTY for session {}", session_id))?;
                Arc::clone(&sess.writer)
            };
            tokio::task::spawn_blocking(move || {
                use std::io::Write;
                let mut w = writer
                    .lock()
                    .map_err(|_| "PTY writer lock poisoned".to_string())?;
                w.write_all(data.as_bytes()).map_err(|e| e.to_string())?;
                w.flush().map_err(|e| e.to_string())?;
                Ok::<(), String>(())
            })
            .await
            .map_err(|e| e.to_string())??;
            Ok(Value::Null)
        }
        "pty_resize" => {
            let session_id = get_str(&args, "sessionId").or_else(|_| get_str(&args, "session_id"))?;
            let cols = args.get("cols").and_then(|v| v.as_u64()).unwrap_or(100) as u16;
            let rows = args.get("rows").and_then(|v| v.as_u64()).unwrap_or(28) as u16;
            let map = ctx.pty_state.sessions.lock().await;
            if let Some(sess) = map.get(&session_id) {
                let _ = sess.master.resize(portable_pty::PtySize {
                    rows: rows.max(4),
                    cols: cols.max(10),
                    pixel_width: 0,
                    pixel_height: 0,
                });
            }
            Ok(Value::Null)
        }
        "pty_kill" => {
            let session_id = get_str(&args, "sessionId").or_else(|_| get_str(&args, "session_id"))?;
            let mut map = ctx.pty_state.sessions.lock().await;
            if let Some(mut sess) = map.remove(&session_id) {
                let _ = sess.child.kill();
            }
            Ok(Value::Null)
        }
        "list_mcp_servers" => {
            let db = ctx.db.lock().await;
            let items = db.list_mcp_servers().map_err(|e| e.to_string())?;
            serde_json::to_value(items).map_err(|e| e.to_string())
        }
        "add_mcp_server" => {
            let name = get_str(&args, "name")?.trim().to_string();
            let transport = get_str(&args, "transport")?.trim().to_lowercase();
            let endpoint = get_str(&args, "endpoint")?.trim().to_string();
            if name.is_empty() {
                return Err("MCP server name cannot be empty".to_string());
            }
            if endpoint.is_empty() {
                return Err("MCP server endpoint cannot be empty".to_string());
            }
            let normalized_transport = if transport == "sse" { "sse" } else { "stdio" };
            let db = ctx.db.lock().await;
            let item = db
                .add_mcp_server(&name, normalized_transport, &endpoint)
                .map_err(|e| e.to_string())?;
            serde_json::to_value(item).map_err(|e| e.to_string())
        }
        "toggle_mcp_server" => {
            let id = get_str(&args, "id")?;
            let enabled = args.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
            let db = ctx.db.lock().await;
            let item = db.toggle_mcp_server(&id, enabled).map_err(|e| e.to_string())?;
            serde_json::to_value(item).map_err(|e| e.to_string())
        }
        "delete_mcp_server" => {
            let id = get_str(&args, "id")?;
            let db = ctx.db.lock().await;
            let ok = db.delete_mcp_server(&id).map_err(|e| e.to_string())?;
            Ok(json!(ok))
        }
        "list_automations" => {
            let db = ctx.db.lock().await;
            let items = db.list_automations().map_err(|e| e.to_string())?;
            serde_json::to_value(items).map_err(|e| e.to_string())
        }
        "create_automation" => {
            let name = get_str(&args, "name")?.trim().to_string();
            let kind_raw = get_str(&args, "kind")?;
            let kind = if kind_raw.trim().eq_ignore_ascii_case("webhook") {
                "webhook"
            } else {
                "cron"
            };
            let schedule = get_str(&args, "schedule")?.trim().to_string();
            let prompt = get_str(&args, "prompt")?.trim().to_string();
            if name.is_empty() || schedule.is_empty() || prompt.is_empty() {
                return Err("Name, schedule, and prompt are required".to_string());
            }
            let db = ctx.db.lock().await;
            let item = db
                .create_automation(&name, kind, &schedule, &prompt)
                .map_err(|e| e.to_string())?;
            serde_json::to_value(item).map_err(|e| e.to_string())
        }
        "toggle_automation" => {
            let id = get_str(&args, "id")?;
            let enabled = args.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
            let db = ctx.db.lock().await;
            let item = db.toggle_automation(&id, enabled).map_err(|e| e.to_string())?;
            serde_json::to_value(item).map_err(|e| e.to_string())
        }
        "delete_automation" => {
            let id = get_str(&args, "id")?;
            let db = ctx.db.lock().await;
            let ok = db.delete_automation(&id).map_err(|e| e.to_string())?;
            Ok(json!(ok))
        }
        "run_automation_now" => {
            let id = get_str(&args, "id")?;
            let item = crate::automation::execute_automation(&ctx.db, ctx.app.as_ref(), &id).await?;
            serde_json::to_value(item).map_err(|e| e.to_string())
        }
        "get_app_customization" => {
            let db = ctx.db.lock().await;
            let cfg = crate::self_control::load_customization(&db);
            serde_json::to_value(cfg).map_err(|e| e.to_string())
        }
        "update_app_customization" => {
            let raw = args.get("customization").cloned().unwrap_or(args);
            let cfg: crate::self_control::AppCustomization =
                serde_json::from_value(raw).map_err(|e| e.to_string())?;
            {
                let db = ctx.db.lock().await;
                crate::self_control::save_customization(&db, &cfg)?;
            }
            crate::platform::emit_app_and_remote(
                ctx.app.as_ref(),
                "sakana:customization-updated",
                &cfg,
            );
            serde_json::to_value(cfg).map_err(|e| e.to_string())
        }
        "reset_app_customization" => {
            let next = {
                let db = ctx.db.lock().await;
                let mut cfg = crate::self_control::load_customization(&db);
                cfg.custom_theme_name = None;
                cfg.css_vars.clear();
                cfg.custom_css.clear();
                cfg.ui_scale = None;
                crate::self_control::save_customization(&db, &cfg)?;
                cfg
            };
            crate::platform::emit_app_and_remote(
                ctx.app.as_ref(),
                "sakana:customization-updated",
                &next,
            );
            serde_json::to_value(next).map_err(|e| e.to_string())
        }
        "list_quota_accounts" => {
            let db = ctx.db.lock().await;
            let items = db.list_accounts().map_err(|e| e.to_string())?;
            serde_json::to_value(items).map_err(|e| e.to_string())
        }
        "list_env_secrets" => {
            let items = crate::telemetry::list_env_secrets().await?;
            serde_json::to_value(items).map_err(|e| e.to_string())
        }
        "get_system_telemetry" => {
            let mut sys = System::new_all();
            sys.refresh_cpu_usage();
            tokio::time::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL).await;
            sys.refresh_cpu_usage();
            sys.refresh_memory();
            let cpu_cores = sys.cpus().len().max(1);
            let cpu_usage_pct = sys.global_cpu_usage();
            let cpu_brand = sys
                .cpus()
                .first()
                .map(|c| c.brand().to_string())
                .unwrap_or_else(|| "CPU".to_string());
            let disks = sysinfo::Disks::new_with_refreshed_list();
            let (mut disk_used, mut disk_total) = (0u64, 0u64);
            for d in disks.list() {
                disk_total = disk_total.saturating_add(d.total_space());
                disk_used = disk_used.saturating_add(d.total_space().saturating_sub(d.available_space()));
            }
            let tel = crate::models::SystemTelemetry {
                os_name: System::name().unwrap_or_else(|| std::env::consts::OS.to_string()),
                os_version: System::os_version().unwrap_or_default(),
                host_name: detect_hostname(),
                cpu_brand,
                cpu_cores,
                cpu_usage_pct,
                memory_used_bytes: sys.used_memory(),
                memory_total_bytes: sys.total_memory(),
                disk_used_bytes: disk_used,
                disk_total_bytes: disk_total,
                uptime_secs: System::uptime(),
                autostart_enabled: false,
            };
            serde_json::to_value(tel).map_err(|e| e.to_string())
        }
        "list_offices" => {
            let db = ctx.db.lock().await;
            let items = db.list_offices().map_err(|e| e.to_string())?;
            serde_json::to_value(items).map_err(|e| e.to_string())
        }
        "get_office" => {
            let id = get_str(&args, "officeId")
                .or_else(|_| get_str(&args, "office_id"))
                .or_else(|_| get_str(&args, "id"))?;
            let db = ctx.db.lock().await;
            let office = db.get_office(&id).map_err(|e| e.to_string())?;
            serde_json::to_value(office).map_err(|e| e.to_string())
        }
        "create_office" => {
            let input_val = args.get("input").cloned().unwrap_or(args);
            let input: CreateOfficeInput =
                serde_json::from_value(input_val).map_err(|e| e.to_string())?;
            let office =
                crate::office::create_office_inner(ctx.app.as_ref(), &ctx.db, input).await?;
            serde_json::to_value(office).map_err(|e| e.to_string())
        }
        "delete_office" => {
            let id = get_str(&args, "officeId")
                .or_else(|_| get_str(&args, "office_id"))
                .or_else(|_| get_str(&args, "id"))?;
            let ok = crate::office::delete_office_inner(
                ctx.app.as_ref(),
                &ctx.db,
                &ctx.office_active,
                &ctx.harness_active,
                id,
            )
            .await?;
            Ok(json!(ok))
        }
        "update_office_settings" => {
            let office_id =
                get_str(&args, "officeId").or_else(|_| get_str(&args, "office_id"))?;
            let max_rounds = args
                .get("maxRounds")
                .or_else(|| args.get("max_rounds"))
                .and_then(|v| v.as_i64())
                .ok_or_else(|| "Missing integer argument 'maxRounds'".to_string())?;
            let office = crate::office::update_office_settings_inner(
                ctx.app.as_ref(),
                &ctx.db,
                office_id,
                max_rounds,
            )
            .await?;
            serde_json::to_value(office).map_err(|e| e.to_string())
        }
        "hire_office_staff" => {
            let input_val = args.get("input").cloned().unwrap_or(args);
            let input: HireStaffInput =
                serde_json::from_value(input_val).map_err(|e| e.to_string())?;
            let staff =
                crate::office::hire_staff_internal(ctx.app.as_ref(), &ctx.db, input).await?;
            serde_json::to_value(staff).map_err(|e| e.to_string())
        }
        "update_office_staff" => {
            let input_val = args.get("input").cloned().unwrap_or(args);
            let input: UpdateStaffInput =
                serde_json::from_value(input_val).map_err(|e| e.to_string())?;
            let staff =
                crate::office::update_office_staff_inner(ctx.app.as_ref(), &ctx.db, input).await?;
            serde_json::to_value(staff).map_err(|e| e.to_string())
        }
        "fire_office_staff" => {
            let id = get_str(&args, "staffId")
                .or_else(|_| get_str(&args, "staff_id"))
                .or_else(|_| get_str(&args, "id"))?;
            let ok = crate::office::fire_office_staff_inner(
                ctx.app.as_ref(),
                &ctx.db,
                &ctx.harness_active,
                id,
            )
            .await?;
            Ok(json!(ok))
        }
        "get_office_staff_messages" => {
            let staff_id = get_str(&args, "staffId")
                .or_else(|_| get_str(&args, "staff_id"))
                .or_else(|_| get_str(&args, "id"))?;
            let db = ctx.db.lock().await;
            let staff = db
                .get_office_staff(&staff_id)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("Office staff '{}' not found", staff_id))?;
            let msgs = db
                .list_messages_for_session(&staff.session_id)
                .map_err(|e| e.to_string())?;
            serde_json::to_value(msgs).map_err(|e| e.to_string())
        }
        "send_office_message" => {
            let office_id =
                get_str(&args, "officeId").or_else(|_| get_str(&args, "office_id"))?;
            let target_staff_id = get_str(&args, "targetStaffId")
                .or_else(|_| get_str(&args, "target_staff_id"))?;
            let prompt = get_str(&args, "prompt")?;
            let attachments = args
                .get("attachments")
                .and_then(|v| v.as_array())
                .cloned();
            crate::office::send_office_message_inner(
                ctx.app.clone(),
                Arc::clone(&ctx.db),
                Arc::clone(&ctx.harness_active),
                Arc::clone(&ctx.office_active),
                office_id,
                target_staff_id,
                prompt,
                attachments,
            )
            .await?;
            Ok(Value::Null)
        }
        "report_staff_to_master" => {
            let office_id =
                get_str(&args, "officeId").or_else(|_| get_str(&args, "office_id"))?;
            let staff_id =
                get_str(&args, "staffId").or_else(|_| get_str(&args, "staff_id"))?;
            let note = get_opt_str(&args, "note");
            let ev = crate::office::report_staff_to_master_inner(
                ctx.app.as_ref(),
                &ctx.db,
                office_id,
                staff_id,
                note,
            )
            .await?;
            serde_json::to_value(ev).map_err(|e| e.to_string())
        }
        "stop_office_run" => {
            let office_id =
                get_str(&args, "officeId").or_else(|_| get_str(&args, "office_id"))?;
            crate::office::stop_office_run_inner(
                ctx.app.as_ref(),
                &ctx.db,
                &ctx.harness_active,
                &ctx.office_active,
                office_id,
            )
            .await?;
            Ok(Value::Null)
        }
        "create_checkpoint" => {
            let session_id = get_str(&args, "sessionId").or_else(|_| get_str(&args, "session_id"))?;
            let turn_number = args
                .get("turnNumber")
                .or_else(|| args.get("turn_number"))
                .and_then(|v| v.as_i64())
                .ok_or_else(|| "Missing 'turnNumber'".to_string())?;
            let label = get_str(&args, "label")?;
            let cwd = get_str(&args, "cwd")?;
            let cp = crate::git::create_checkpoint_inner(&ctx.db, session_id, turn_number, label, cwd).await?;
            serde_json::to_value(cp).map_err(|e| e.to_string())
        }
        "get_checkpoint_diff" => {
            let session_id = get_str(&args, "sessionId").or_else(|_| get_str(&args, "session_id"))?;
            let checkpoint_id = get_str(&args, "checkpointId").or_else(|_| get_str(&args, "checkpoint_id"))?;
            let diffs = crate::git::get_checkpoint_diff_inner(&ctx.db, session_id, checkpoint_id).await?;
            serde_json::to_value(diffs).map_err(|e| e.to_string())
        }
        "revert_to_checkpoint" => {
            let session_id = get_str(&args, "sessionId").or_else(|_| get_str(&args, "session_id"))?;
            let checkpoint_id = get_str(&args, "checkpointId").or_else(|_| get_str(&args, "checkpoint_id"))?;
            crate::git::revert_to_checkpoint_inner(&ctx.db, session_id, checkpoint_id).await?;
            Ok(Value::Null)
        }
        "list_trace_spans" => {
            let session_id = get_str(&args, "sessionId").or_else(|_| get_str(&args, "session_id"))?;
            let spans = crate::git::list_trace_spans_inner(&ctx.db, session_id).await?;
            serde_json::to_value(spans).map_err(|e| e.to_string())
        }
        "create_harness_account" => {
            let harness = get_str(&args, "harness")?;
            let label = get_str(&args, "label")?;
            let email = get_opt_str(&args, "email");
            let auth_type = get_opt_str(&args, "authType").or_else(|| get_opt_str(&args, "auth_type"));
            let accounts = crate::auth::create_harness_account_inner(
                &ctx.db,
                &ctx.auth_state,
                harness,
                label,
                email,
                auth_type,
            )
            .await?;
            serde_json::to_value(accounts).map_err(|e| e.to_string())
        }
        "delete_harness_account" => {
            let account_id = get_str(&args, "accountId").or_else(|_| get_str(&args, "account_id"))?;
            let accounts = crate::auth::delete_harness_account_inner(&ctx.db, &ctx.auth_state, account_id).await?;
            serde_json::to_value(accounts).map_err(|e| e.to_string())
        }
        "set_default_harness_account" => {
            let account_id = get_str(&args, "accountId").or_else(|_| get_str(&args, "account_id"))?;
            let accounts = crate::auth::set_default_harness_account_inner(&ctx.db, &ctx.auth_state, account_id).await?;
            serde_json::to_value(accounts).map_err(|e| e.to_string())
        }
        "rename_harness_account" => {
            let account_id = get_str(&args, "accountId").or_else(|_| get_str(&args, "account_id"))?;
            let label = get_str(&args, "label")?;
            let accounts = crate::auth::rename_harness_account_inner(&ctx.db, &ctx.auth_state, account_id, label).await?;
            serde_json::to_value(accounts).map_err(|e| e.to_string())
        }
        "check_harness_auth" => {
            let account_id = get_str(&args, "accountId").or_else(|_| get_str(&args, "account_id"))?;
            let account = crate::auth::check_harness_auth_inner(&ctx.db, &ctx.auth_state, account_id).await?;
            serde_json::to_value(account).map_err(|e| e.to_string())
        }
        "start_harness_login" => {
            let account_id = get_str(&args, "accountId").or_else(|_| get_str(&args, "account_id"))?;
            let auth_type = get_opt_str(&args, "authType").or_else(|| get_opt_str(&args, "auth_type"));
            let mode = get_opt_str(&args, "mode");
            let res = crate::auth::start_harness_login_inner(&ctx.db, &ctx.auth_state, account_id, auth_type, mode).await?;
            serde_json::to_value(res).map_err(|e| e.to_string())
        }
        "submit_harness_login_code" => {
            let account_id = get_str(&args, "accountId").or_else(|_| get_str(&args, "account_id"))?;
            let code = get_str(&args, "code")?;
            let res = crate::auth::submit_harness_login_code_inner(&ctx.db, &ctx.auth_state, account_id, code).await?;
            serde_json::to_value(res).map_err(|e| e.to_string())
        }
        "logout_harness_account" => {
            let account_id = get_str(&args, "accountId").or_else(|_| get_str(&args, "account_id"))?;
            let accounts = crate::auth::logout_harness_account_inner(&ctx.db, &ctx.auth_state, account_id).await?;
            serde_json::to_value(accounts).map_err(|e| e.to_string())
        }
        "set_env_secret" => {
            let key = get_str(&args, "key")?;
            let value = get_str(&args, "value")?;
            let secrets = crate::telemetry::set_env_secret_inner(key, value).await?;
            serde_json::to_value(secrets).map_err(|e| e.to_string())
        }
        "delete_env_secret" => {
            let key = get_str(&args, "key")?;
            let secrets = crate::telemetry::delete_env_secret_inner(key).await?;
            serde_json::to_value(secrets).map_err(|e| e.to_string())
        }
        "reveal_env_secret" => {
            let key = get_str(&args, "key")?;
            let val = crate::telemetry::reveal_env_secret_inner(key).await?;
            Ok(json!(val))
        }
        "upsert_quota_account" => {
            let id = get_opt_str(&args, "id");
            let harness = get_opt_str(&args, "harness");
            let provider = get_opt_str(&args, "provider");
            let label = get_str(&args, "label")?;
            let email = get_opt_str(&args, "email");
            let plan = get_opt_str(&args, "plan");
            let five_hour_pct = args.get("fiveHourPct").or_else(|| args.get("five_hour_pct")).and_then(|v| v.as_f64());
            let five_hour_reset = get_opt_str(&args, "fiveHourReset").or_else(|| get_opt_str(&args, "five_hour_reset"));
            let weekly_pct = args.get("weeklyPct").or_else(|| args.get("weekly_pct")).and_then(|v| v.as_f64());
            let weekly_reset = get_opt_str(&args, "weeklyReset").or_else(|| get_opt_str(&args, "weekly_reset"));
            let is_default = args.get("isDefault").or_else(|| args.get("is_default")).and_then(|v| v.as_bool());
            let accounts = crate::telemetry::upsert_quota_account_inner(
                &ctx.db,
                id,
                harness,
                provider,
                label,
                email,
                plan,
                five_hour_pct,
                five_hour_reset,
                weekly_pct,
                weekly_reset,
                is_default,
            )
            .await?;
            serde_json::to_value(accounts).map_err(|e| e.to_string())
        }
        "delete_quota_account" => {
            let id = get_str(&args, "id")?;
            let accounts = crate::telemetry::delete_quota_account_inner(&ctx.db, id).await?;
            serde_json::to_value(accounts).map_err(|e| e.to_string())
        }
        "ping_mcp_server" => {
            let id = get_str(&args, "id")?;
            let server = crate::mcp::ping_mcp_server_inner(&ctx.db, id).await?;
            serde_json::to_value(server).map_err(|e| e.to_string())
        }
        "import_agy_mcp_servers" => {
            let servers = crate::mcp::import_agy_mcp_servers_inner(&ctx.db).await?;
            serde_json::to_value(servers).map_err(|e| e.to_string())
        }
        "save_clipboard_attachment" => {
            let name = get_str(&args, "name")?;
            let mime = get_opt_str(&args, "mime").unwrap_or_default();
            let data_base64 = get_str(&args, "dataBase64").or_else(|_| get_str(&args, "data_base64"))?;
            let item = crate::save_clipboard_attachment_inner(name, mime, data_base64)?;
            serde_json::to_value(item).map_err(|e| e.to_string())
        }
        "translate_nl_command" => {
            let prompt = get_str(&args, "prompt")?;
            let os = get_str(&args, "os")?;
            let shell = get_str(&args, "shell")?;
            let cmd = crate::pty::translate_nl_command_inner(&ctx.db, prompt, os, shell).await?;
            Ok(json!(cmd))
        }
        "get_discord_status" => {
            let status = crate::discord::get_discord_status_inner(&ctx.discord_state, &ctx.db).await?;
            serde_json::to_value(status).map_err(|e| e.to_string())
        }
        "start_discord_bot" => {
            let token = get_opt_str(&args, "token");
            let status = crate::discord::start_discord_bot_inner(
                &ctx.discord_state,
                &ctx.db,
                ctx.app.clone(),
                token,
            )
            .await?;
            serde_json::to_value(status).map_err(|e| e.to_string())
        }
        "stop_discord_bot" => {
            let status = crate::discord::stop_discord_bot_inner(&ctx.discord_state, &ctx.db).await?;
            serde_json::to_value(status).map_err(|e| e.to_string())
        }
        "upsert_discord_rule" => {
            let user_id = get_str(&args, "userId").or_else(|_| get_str(&args, "user_id"))?;
            let label = get_str(&args, "label")?;
            let role = get_str(&args, "role")?;
            let rule = crate::discord::upsert_discord_rule_inner(&ctx.db, user_id, label, role).await?;
            serde_json::to_value(rule).map_err(|e| e.to_string())
        }
        "delete_discord_rule" => {
            let id = get_str(&args, "id")?;
            let ok = crate::discord::delete_discord_rule_inner(&ctx.db, id).await?;
            Ok(json!(ok))
        }
        "get_discord_settings" => {
            let cfg = crate::discord::get_discord_settings_inner(&ctx.db).await?;
            serde_json::to_value(cfg).map_err(|e| e.to_string())
        }
        "update_discord_settings" => {
            let raw = args.get("settings").cloned().unwrap_or(args);
            let cfg: crate::models::DiscordSettings =
                serde_json::from_value(raw).map_err(|e| e.to_string())?;
            let saved = crate::discord::update_discord_settings_inner(&ctx.db, cfg).await?;
            serde_json::to_value(saved).map_err(|e| e.to_string())
        }
        other => Err(format!("Unsupported remote RPC command: '{}'", other)),
    }
}

async fn api_rpc(
    AxumState(ctx): AxumState<ServerContext>,
    AxumPath(cmd): AxumPath<String>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
    Json(args): Json<Value>,
) -> impl IntoResponse {
    if !is_authorized(&headers, &query, &ctx.token, false) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Invalid or missing Sakana token" })),
        )
            .into_response();
    }
    match dispatch_rpc(&ctx, &cmd, args).await {
        Ok(val) => (StatusCode::OK, Json(val)).into_response(),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": err })),
        )
            .into_response(),
    }
}

async fn api_webhook(
    AxumState(ctx): AxumState<ServerContext>,
    AxumPath(hook_path): AxumPath<String>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
    body: String,
) -> impl IntoResponse {
    if !is_authorized(&headers, &query, &ctx.token, false) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Invalid or missing Sakana token" })),
        )
            .into_response();
    }
    let payload = if body.trim().is_empty() { None } else { Some(body) };
    match crate::automation::trigger_webhook_by_path(
        Arc::clone(&ctx.db),
        ctx.app.clone(),
        &hook_path,
        payload,
    )
    .await
    {
        Ok(triggered) => (
            StatusCode::OK,
            Json(json!({ "ok": true, "path": hook_path, "triggered": triggered })),
        )
            .into_response(),
        Err(err) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "ok": false, "error": err })),
        )
            .into_response(),
    }
}

fn is_private_or_tailscale_host(host: &str) -> bool {
    if host == "localhost" || host == "127.0.0.1" || host.ends_with(".ts.net") {
        return true;
    }
    if let Ok(ip) = host.parse::<std::net::Ipv4Addr>() {
        let [a, b, _, _] = ip.octets();
        // Loopback, Tailscale CGNAT (100.64.0.0/10), RFC1918 LAN ranges
        if a == 127
            || (a == 100 && (64..=127).contains(&b))
            || a == 10
            || (a == 192 && b == 168)
            || (a == 172 && (16..=31).contains(&b))
        {
            return true;
        }
    }
    false
}

fn is_allowed_origin(origin: &HeaderValue) -> bool {
    let Ok(s) = origin.to_str() else {
        return false;
    };
    let lower = s.trim().to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "tauri://localhost" | "http://tauri.localhost" | "https://tauri.localhost"
    ) {
        return true;
    }
    let rest = lower
        .strip_prefix("http://")
        .or_else(|| lower.strip_prefix("https://"));
    if let Some(authority) = rest {
        let (host, port_ok) = match authority.split_once(':') {
            Some((h, p)) => (h, !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())),
            None => (authority, true),
        };
        if port_ok && is_private_or_tailscale_host(host) {
            return true;
        }
    }
    false
}

pub fn build_remote_router(ctx: ServerContext) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(|origin, _req| {
            is_allowed_origin(origin)
        }))
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            HeaderName::from_static("x-sakana-token"),
        ]);

    Router::new()
        .route("/api/health", get(api_health))
        .route("/api/host-info", get(api_host_info))
        .route("/api/bootstrap", get(api_bootstrap))
        .route("/api/events", get(api_events))
        .route("/api/rpc/{cmd}", post(api_rpc))
        .route("/hooks/{*path}", post(api_webhook))
        .route("/api/hooks/{*path}", post(api_webhook))
        .layer(cors)
        .with_state(ctx)
}

/// Run the Sakana Remote RPC + SSE server (used by headless `sakana-daemon`).
pub async fn serve_remote_daemon(
    db: Arc<Mutex<Database>>,
    port: u16,
    token: String,
) -> Result<(), String> {
    let lan_enabled = std::env::var("SAKANA_REMOTE_LAN")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let host = if lan_enabled { "0.0.0.0" } else { "127.0.0.1" };
    let bind_addr = format!("{}:{}", host, port);
    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .map_err(|e| format!("Failed to bind {}: {}", bind_addr, e))?;
    let ctx = ServerContext {
        db,
        harness_active: Arc::new(Mutex::new(HashMap::new())),
        office_active: Arc::new(Mutex::new(HashMap::new())),
        pty_state: Arc::new(PtyState::default()),
        auth_state: Arc::new(AuthState::default()),
        discord_state: Arc::new(crate::discord::DiscordState::default()),
        app: None,
        token,
        port,
        host_id: format!("daemon-{}", &Uuid::new_v4().simple().to_string()[..8]),
    };
    let router = build_remote_router(ctx);
    axum::serve(listener, router)
        .await
        .map_err(|e| format!("Remote daemon error: {}", e))
}

#[tauri::command]
pub async fn get_remote_status(
    state: State<'_, RemoteState>,
    db_state: State<'_, DbState>,
) -> Result<RemoteStatus, String> {
    let fallback = state.token.lock().await.clone();
    let persisted = {
        let db = db_state.0.lock().await;
        load_or_init_remote_token(&db, &fallback)
    };
    *state.token.lock().await = persisted;
    Ok(build_remote_status(&state).await)
}

#[tauri::command]
pub async fn start_remote_server(
    app: AppHandle,
    state: State<'_, RemoteState>,
    db_state: State<'_, DbState>,
    harness_state: State<'_, HarnessState>,
    office_state: State<'_, crate::office::OfficeState>,
    pty_state: State<'_, PtyState>,
    auth_state: State<'_, AuthState>,
    discord_state: State<'_, crate::discord::DiscordState>,
    port: Option<u16>,
    lan_enabled: Option<bool>,
) -> Result<RemoteStatus, String> {
    if *state.running.lock().await {
        return Ok(build_remote_status(&state).await);
    }
    let target_port = port.unwrap_or(DEFAULT_REMOTE_PORT);
    let lan = lan_enabled.unwrap_or_else(|| {
        std::env::var("SAKANA_REMOTE_LAN")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    });
    let host = if lan { "0.0.0.0" } else { "127.0.0.1" };
    let bind_addr = format!("{}:{}", host, target_port);
    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .map_err(|e| format!("Failed to bind {}: {}", bind_addr, e))?;

    let fallback = state.token.lock().await.clone();
    let token = {
        let db = db_state.0.lock().await;
        load_or_init_remote_token(&db, &fallback)
    };
    *state.token.lock().await = token.clone();
    *state.bind_addr.lock().await = host.to_string();
    let ctx = ServerContext {
        db: Arc::clone(&db_state.0),
        harness_active: Arc::clone(&harness_state.active_turns),
        office_active: Arc::clone(&office_state.active_runs),
        pty_state: Arc::new(PtyState {
            sessions: Arc::clone(&pty_state.sessions),
        }),
        auth_state: Arc::new(AuthState {
            login_procs: Arc::clone(&auth_state.login_procs),
            last_outputs: Arc::clone(&auth_state.last_outputs),
        }),
        discord_state: Arc::new(crate::discord::DiscordState {
            running: Arc::clone(&discord_state.running),
            bot_tag: Arc::clone(&discord_state.bot_tag),
            connected_guilds: Arc::clone(&discord_state.connected_guilds),
            last_error: Arc::clone(&discord_state.last_error),
            shard_manager: Arc::clone(&discord_state.shard_manager),
        }),
        app: Some(app.clone()),
        token,
        port: target_port,
        host_id: state.host_id.clone(),
    };
    let app_router = build_remote_router(ctx);

    let (tx, rx) = oneshot::channel::<()>();
    *state.shutdown_tx.lock().await = Some(tx);
    *state.port.lock().await = target_port;
    *state.running.lock().await = true;
    *state.last_error.lock().await = None;

    let running_flag = Arc::clone(&state.running);
    let last_err = Arc::clone(&state.last_error);

    tokio::spawn(async move {
        let server = axum::serve(listener, app_router).with_graceful_shutdown(async move {
            let _ = rx.await;
        });
        if let Err(err) = server.await {
            *last_err.lock().await = Some(err.to_string());
        }
        *running_flag.lock().await = false;
    });

    Ok(build_remote_status(&state).await)
}

#[tauri::command]
pub async fn stop_remote_server(state: State<'_, RemoteState>) -> Result<RemoteStatus, String> {
    if let Some(tx) = state.shutdown_tx.lock().await.take() {
        let _ = tx.send(());
    }
    *state.running.lock().await = false;
    Ok(build_remote_status(&state).await)
}

// --- Client Mode: Link and control remote Sakana instances ---

#[tauri::command]
pub async fn list_remote_hosts(db_state: State<'_, DbState>) -> Result<Vec<RemoteHost>, String> {
    let db = db_state.0.lock().await;
    db.list_remote_hosts().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn upsert_remote_host(
    db_state: State<'_, DbState>,
    input: UpsertRemoteHostInput,
) -> Result<Vec<RemoteHost>, String> {
    let db = db_state.0.lock().await;
    db.upsert_remote_host(input).map_err(|e| e.to_string())?;
    db.list_remote_hosts().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_remote_host(
    db_state: State<'_, DbState>,
    id: String,
) -> Result<Vec<RemoteHost>, String> {
    let db = db_state.0.lock().await;
    db.delete_remote_host(&id).map_err(|e| e.to_string())?;
    db.list_remote_hosts().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn test_remote_host(
    db_state: State<'_, DbState>,
    remote_state: State<'_, RemoteState>,
    id: String,
) -> Result<HostInfo, String> {
    let host = {
        let db = db_state.0.lock().await;
        db.get_remote_host(&id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Remote host '{}' not found", id))?
    };
    let url = format!("{}/api/host-info", host.url.trim_end_matches('/'));
    let resp = remote_state
        .http_client
        .get(&url)
        .bearer_auth(&host.token)
        .send()
        .await
        .map_err(|e| {
            let db_err = format!("Connection failed: {}", e);
            db_err
        });

    match resp {
        Ok(r) if r.status().is_success() => {
            let info: HostInfo = r.json().await.map_err(|e| format!("Invalid host-info: {}", e))?;
            let db = db_state.0.lock().await;
            let _ = db.update_remote_host_status(&id, "online", Some(&info.os), Some(&info.hostname));
            Ok(info)
        }
        Ok(r) => {
            let status = r.status();
            let text = r.text().await.unwrap_or_default();
            let db = db_state.0.lock().await;
            let _ = db.update_remote_host_status(&id, "offline", None, None);
            Err(format!("Remote host returned {}: {}", status, text))
        }
        Err(err) => {
            let db = db_state.0.lock().await;
            let _ = db.update_remote_host_status(&id, "offline", None, None);
            Err(err)
        }
    }
}

#[tauri::command]
pub async fn remote_rpc(
    db_state: State<'_, DbState>,
    remote_state: State<'_, RemoteState>,
    host_id: String,
    cmd: String,
    args: Option<Value>,
) -> Result<Value, String> {
    let host = {
        let db = db_state.0.lock().await;
        db.get_remote_host(&host_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Remote host '{}' not found", host_id))?
    };
    let url = format!("{}/api/rpc/{}", host.url.trim_end_matches('/'), cmd);
    let resp = remote_state
        .http_client
        .post(&url)
        .bearer_auth(&host.token)
        .json(&args.unwrap_or_else(|| json!({})))
        .send()
        .await
        .map_err(|e| format!("Remote RPC '{}' failed: {}", cmd, e))?;
    let status = resp.status();
    let body: Value = resp
        .json()
        .await
        .map_err(|e| format!("Invalid JSON from remote '{}': {}", cmd, e))?;
    if !status.is_success() {
        let msg = body
            .get("error")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("Remote HTTP {}", status));
        return Err(msg);
    }
    Ok(body)
}

#[tauri::command]
pub async fn disconnect_remote_events(remote_state: State<'_, RemoteState>) -> Result<(), String> {
    if let Some(tx) = remote_state.event_client_stop.lock().await.take() {
        let _ = tx.send(());
    }
    Ok(())
}

#[tauri::command]
pub async fn connect_remote_events(
    app: AppHandle,
    db_state: State<'_, DbState>,
    remote_state: State<'_, RemoteState>,
    host_id: String,
) -> Result<(), String> {
    // Stop any previous SSE listener
    if let Some(tx) = remote_state.event_client_stop.lock().await.take() {
        let _ = tx.send(());
    }
    let host = {
        let db = db_state.0.lock().await;
        db.get_remote_host(&host_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Remote host '{}' not found", host_id))?
    };
    let (stop_tx, mut stop_rx) = oneshot::channel::<()>();
    *remote_state.event_client_stop.lock().await = Some(stop_tx);

    let url = format!("{}/api/events", host.url.trim_end_matches('/'));
    let token = host.token.clone();
    let client = reqwest::Client::new();

    tokio::spawn(async move {
        loop {
            let req = client.get(&url).bearer_auth(&token).send();
            let resp = tokio::select! {
                _ = &mut stop_rx => break,
                r = req => r,
            };
            let Ok(resp) = resp else {
                tokio::select! {
                    _ = &mut stop_rx => break,
                    _ = tokio::time::sleep(Duration::from_secs(3)) => continue,
                }
            };
            if !resp.status().is_success() {
                tokio::select! {
                    _ = &mut stop_rx => break,
                    _ = tokio::time::sleep(Duration::from_secs(5)) => continue,
                }
            }
            let mut stream = resp.bytes_stream();
            let mut pending_utf8: Vec<u8> = Vec::new();
            let mut buf = String::new();
            loop {
                let chunk = tokio::select! {
                    _ = &mut stop_rx => return,
                    next = stream.next() => next,
                };
                match chunk {
                    Some(Ok(bytes)) => {
                        pending_utf8.extend_from_slice(&bytes);
                        drain_valid_utf8(&mut pending_utf8, &mut buf);
                        while let Some(pos) = buf.find("\n\n") {
                            let block = buf[..pos].to_string();
                            buf = buf[pos + 2..].to_string();
                            for line in block.lines() {
                                if let Some(data) = line.strip_prefix("data:") {
                                    if let Ok(val) = serde_json::from_str::<Value>(data.trim()) {
                                        if let Some(ev_name) = val.get("event").and_then(|v| v.as_str()) {
                                            let payload = val.get("payload").cloned().unwrap_or(Value::Null);
                                            let _ = app.emit(ev_name, payload);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ => break,
                }
            }
            tokio::select! {
                _ = &mut stop_rx => break,
                _ = tokio::time::sleep(Duration::from_secs(2)) => continue,
            }
        }
    });
    Ok(())
}

/// Decode complete UTF-8 sequences from `pending` into `out`, leaving any
/// incomplete trailing multi-byte sequence in `pending` for the next chunk.
fn drain_valid_utf8(pending: &mut Vec<u8>, out: &mut String) {
    loop {
        match std::str::from_utf8(pending) {
            Ok(valid) => {
                out.push_str(valid);
                pending.clear();
                break;
            }
            Err(err) => {
                let valid_up_to = err.valid_up_to();
                if valid_up_to > 0 {
                    if let Ok(prefix) = std::str::from_utf8(&pending[..valid_up_to]) {
                        out.push_str(prefix);
                    }
                }
                if let Some(error_len) = err.error_len() {
                    out.push('\u{FFFD}');
                    pending.drain(..valid_up_to + error_len);
                } else {
                    pending.drain(..valid_up_to);
                    break;
                }
            }
        }
    }
}

async fn execute_remote_automation(
    db: &Arc<Mutex<Database>>,
    app: Option<&AppHandle>,
    id: &str,
) -> Result<crate::models::AutomationItem, String> {
    let auto = {
        let db_lock = db.lock().await;
        db_lock
            .get_automation(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Automation '{id}' not found"))?
    };
    let (cwd, claude_acc, agy_acc) = {
        let db_lock = db.lock().await;
        let cwd = db_lock
            .list_projects()
            .ok()
            .and_then(|projects| projects.into_iter().next())
            .map(|p| p.cwd);
        let claude_acc = db_lock.get_default_harness_account("claude-code").ok().flatten();
        let agy_acc = db_lock.get_default_harness_account("agy-cli").ok().flatten();
        (cwd, claude_acc, agy_acc)
    };

    let start = std::time::Instant::now();
    let trimmed = auto.prompt.trim();
    let (status, summary) = if trimmed.starts_with("cmd:") || trimmed.starts_with("$ ") {
        let cmd_str = trimmed
            .strip_prefix("cmd:")
            .or_else(|| trimmed.strip_prefix("$ "))
            .unwrap_or(trimmed)
            .trim();
        #[cfg(target_os = "windows")]
        let mut cmd = {
            let mut c = tokio::process::Command::new("cmd");
            c.args(["/C", cmd_str]);
            c.creation_flags(0x0800_0000);
            c
        };
        #[cfg(not(target_os = "windows"))]
        let mut cmd = {
            let mut c = tokio::process::Command::new("sh");
            c.args(["-c", cmd_str]);
            c
        };
        if let Some(ref dir) = cwd.as_deref().filter(|c| std::path::Path::new(c).is_dir()) {
            cmd.current_dir(dir);
        }
        match cmd
            .stdin(std::process::Stdio::null())
            .output()
            .await
        {
            Ok(out) if out.status.success() => {
                let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
                let s = if text.is_empty() {
                    format!("Command '{}' succeeded", cmd_str)
                } else {
                    text.chars().take(240).collect()
                };
                ("OK".to_string(), s)
            }
            Ok(out) => {
                let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
                let s = if err.is_empty() {
                    format!("Command exited with {}", out.status)
                } else {
                    err.chars().take(240).collect()
                };
                ("ERR".to_string(), s)
            }
            Err(e) => ("ERR".to_string(), format!("Failed to spawn shell: {e}")),
        }
    } else if let Some((harness_id, bin, acc)) = crate::harness::resolve_harness_binary("claude-code")
        .map(|bin| ("claude-code", bin, claude_acc.as_ref()))
        .or_else(|| {
            crate::harness::resolve_harness_binary("agy-cli")
                .map(|bin| ("agy-cli", bin, agy_acc.as_ref()))
        })
    {
        let enabled_mcp_servers: Vec<crate::models::McpServer> = {
            let db_lock = db.lock().await;
            db_lock
                .list_mcp_servers()
                .unwrap_or_default()
                .into_iter()
                .filter(|s| s.enabled)
                .collect()
        };
        crate::auth::sync_profile_mcp_and_skills(harness_id, acc, &enabled_mcp_servers);
        let bin_str = bin.to_string_lossy().to_string();
        let mut cmd = crate::platform::tokio_command(&bin_str);
        if let Some(ref dir) = cwd.as_deref().filter(|c| std::path::Path::new(c).is_dir()) {
            cmd.current_dir(dir);
        }
        crate::auth::apply_account_env(&mut cmd, harness_id, acc);
        cmd.args(["-p", trimmed]);
        cmd.args(crate::harness::build_permission_args(harness_id, "yolo"));
        cmd.kill_on_drop(true)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        match tokio::time::timeout(Duration::from_secs(300), cmd.output()).await {
            Ok(Ok(out)) if out.status.success() => {
                let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
                let s = if text.is_empty() {
                    format!("Completed '{}' via {}", auto.name, bin.display())
                } else {
                    text.chars().take(240).collect()
                };
                ("OK".to_string(), s)
            }
            Ok(Ok(out)) => {
                let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
                let s = if err.is_empty() {
                    format!("Harness exited with {}", out.status)
                } else {
                    err.chars().take(240).collect()
                };
                ("ERR".to_string(), s)
            }
            Ok(Err(e)) => ("ERR".to_string(), format!("Failed to spawn harness: {e}")),
            Err(_) => ("ERR".to_string(), "Automation timed out after 300s".to_string()),
        }
    } else {
        (
            "ERR".to_string(),
            format!("No CLI harness (claude/agy) found on PATH to run '{}'", auto.name),
        )
    };

    let duration_ms = start.elapsed().as_millis().max(1) as i64;
    let updated = {
        let db_lock = db.lock().await;
        db_lock
            .record_automation_run(id, &status, duration_ms, &summary)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Automation '{id}' not found after run"))?
    };
    crate::platform::emit_app_and_remote(app, "automation-run", &updated);
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_time_eq_handles_equal_and_unequal_tokens() {
        assert!(constant_time_eq("secret-token-123", "secret-token-123"));
        assert!(!constant_time_eq("secret-token-123", "secret-token-124"));
        assert!(!constant_time_eq("short", "longer-token"));
        assert!(constant_time_eq("", ""));
    }

    #[test]
    fn is_authorized_rejects_query_token_when_disallowed() {
        let mut headers = HeaderMap::new();
        let mut query = HashMap::new();
        query.insert("token".to_string(), "tok-abc".to_string());

        assert!(!is_authorized(&headers, &query, "tok-abc", false));
        assert!(is_authorized(&headers, &query, "tok-abc", true));

        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer tok-abc"),
        );
        assert!(is_authorized(&headers, &HashMap::new(), "tok-abc", false));

        let mut x_headers = HeaderMap::new();
        x_headers.insert("x-sakana-token", HeaderValue::from_static("tok-abc"));
        assert!(is_authorized(&x_headers, &HashMap::new(), "tok-abc", false));
    }

    #[test]
    fn cors_origin_predicate_allows_only_localhost_and_tauri() {
        assert!(is_allowed_origin(&HeaderValue::from_static("http://localhost:5173")));
        assert!(is_allowed_origin(&HeaderValue::from_static("http://127.0.0.1:4317")));
        assert!(is_allowed_origin(&HeaderValue::from_static("tauri://localhost")));
        assert!(is_allowed_origin(&HeaderValue::from_static("https://tauri.localhost")));
        assert!(!is_allowed_origin(&HeaderValue::from_static("https://evil.example.com")));
        assert!(!is_allowed_origin(&HeaderValue::from_static("http://localhost.evil.com")));
    }

    #[test]
    fn remote_token_persists_in_sqlite_settings_kv() {
        let temp_dir = std::env::temp_dir().join(format!("sakana-remote-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let db = Database::open_in_dir(&temp_dir).unwrap();
        let t1 = load_or_init_remote_token(&db, "initial-tok-1");
        assert_eq!(t1, "initial-tok-1");
        let t2 = load_or_init_remote_token(&db, "different-tok-2");
        assert_eq!(t2, "initial-tok-1");
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn drain_valid_utf8_preserves_split_multibyte_chars() {
        let full = "data: {\"text\":\"🐟日本語\"}\n\n";
        let bytes = full.as_bytes();
        // Split right in the middle of the 4-byte fish emoji
        let split_at = "data: {\"text\":\"".len() + 2;
        let mut pending = Vec::new();
        let mut out = String::new();

        pending.extend_from_slice(&bytes[..split_at]);
        drain_valid_utf8(&mut pending, &mut out);
        assert_eq!(out, "data: {\"text\":\"");
        assert_eq!(pending.len(), 2);

        pending.extend_from_slice(&bytes[split_at..]);
        drain_valid_utf8(&mut pending, &mut out);
        assert_eq!(out, full);
        assert!(pending.is_empty());
    }

    fn make_test_context(temp_dir: &std::path::Path) -> ServerContext {
        let db = Database::open_in_dir(temp_dir).unwrap();
        ServerContext {
            db: Arc::new(Mutex::new(db)),
            harness_active: Arc::new(Mutex::new(HashMap::new())),
            office_active: Arc::new(Mutex::new(HashMap::new())),
            pty_state: Arc::new(PtyState::default()),
            auth_state: Arc::new(AuthState::default()),
            discord_state: Arc::new(crate::discord::DiscordState::default()),
            app: None,
            token: "test-token".to_string(),
            port: 0,
            host_id: "test-host".to_string(),
        }
    }

    #[tokio::test]
    async fn dispatch_rpc_office_lifecycle_headless() {
        let temp_dir =
            std::env::temp_dir().join(format!("sakana-remote-office-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let ctx = make_test_context(&temp_dir);

        // create_office
        let created = dispatch_rpc(
            &ctx,
            "create_office",
            json!({
                "input": {
                    "name": "Remote Office",
                    "masterHarness": "claude",
                    "masterModel": "claude-sonnet-4-5"
                }
            }),
        )
        .await
        .unwrap();
        let office_id = created["id"].as_str().unwrap().to_string();
        assert_eq!(created["name"], "Remote Office");

        // list_offices
        let list = dispatch_rpc(&ctx, "list_offices", json!({})).await.unwrap();
        let arr = list.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["id"], office_id);

        // get_office
        let fetched = dispatch_rpc(&ctx, "get_office", json!({ "officeId": office_id }))
            .await
            .unwrap();
        assert_eq!(fetched["id"], office_id);

        // update_office_settings
        let updated_settings = dispatch_rpc(
            &ctx,
            "update_office_settings",
            json!({ "officeId": office_id, "maxRounds": 7 }),
        )
        .await
        .unwrap();
        assert_eq!(updated_settings["maxRounds"], 7);

        // hire_office_staff
        let hired = dispatch_rpc(
            &ctx,
            "hire_office_staff",
            json!({
                "input": {
                    "officeId": office_id,
                    "name": "Worker One",
                    "role": "engineer",
                    "harness": "claude",
                    "model": "claude-sonnet-4-5"
                }
            }),
        )
        .await
        .unwrap();
        let staff_id = hired["id"].as_str().unwrap().to_string();
        assert_eq!(hired["name"], "Worker One");

        // update_office_staff
        let updated_staff = dispatch_rpc(
            &ctx,
            "update_office_staff",
            json!({
                "input": {
                    "id": staff_id,
                    "name": "Worker Updated",
                    "role": "senior-engineer"
                }
            }),
        )
        .await
        .unwrap();
        assert_eq!(updated_staff["name"], "Worker Updated");
        assert_eq!(updated_staff["role"], "senior-engineer");

        // get_office_staff_messages
        let msgs = dispatch_rpc(
            &ctx,
            "get_office_staff_messages",
            json!({ "staffId": staff_id }),
        )
        .await
        .unwrap();
        assert!(msgs.as_array().unwrap().is_empty());

        // fire_office_staff
        let fired = dispatch_rpc(
            &ctx,
            "fire_office_staff",
            json!({ "staffId": staff_id }),
        )
        .await
        .unwrap();
        assert_eq!(fired, json!(true));

        // delete_office
        let deleted = dispatch_rpc(
            &ctx,
            "delete_office",
            json!({ "officeId": office_id }),
        )
        .await
        .unwrap();
        assert_eq!(deleted, json!(true));

        let after_delete = dispatch_rpc(&ctx, "list_offices", json!({})).await.unwrap();
        assert!(after_delete.as_array().unwrap().is_empty());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn dispatch_rpc_checkpoints_headless() {
        let temp_dir =
            std::env::temp_dir().join(format!("sakana-remote-cp-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let git_init = std::process::Command::new("git")
            .args(["init"])
            .current_dir(&temp_dir)
            .output()
            .unwrap();
        assert!(git_init.status.success());
        let _ = std::process::Command::new("git")
            .args(["config", "user.email", "test@sakana.local"])
            .current_dir(&temp_dir)
            .output();
        let _ = std::process::Command::new("git")
            .args(["config", "user.name", "Sakana Test"])
            .current_dir(&temp_dir)
            .output();
        std::fs::write(temp_dir.join("README.md"), "init\n").unwrap();
        let _ = std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&temp_dir)
            .output();
        let git_commit = std::process::Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(&temp_dir)
            .output()
            .unwrap();
        assert!(git_commit.status.success());

        let ctx = make_test_context(&temp_dir);

        let proj = dispatch_rpc(
            &ctx,
            "create_project",
            json!({
                "name": "Checkpoint Test Project",
                "cwd": temp_dir.to_string_lossy().to_string(),
            }),
        )
        .await
        .unwrap();
        let project_id = proj["id"].as_str().unwrap().to_string();

        let sess = dispatch_rpc(
            &ctx,
            "create_session",
            json!({
                "projectId": project_id,
                "title": "Test Session",
                "harness": "claude",
            }),
        )
        .await
        .unwrap();
        let session_id = sess["id"].as_str().unwrap().to_string();

        let spans = dispatch_rpc(
            &ctx,
            "list_trace_spans",
            json!({ "sessionId": session_id }),
        )
        .await
        .unwrap();
        assert!(spans.as_array().unwrap().is_empty());

        let cp = dispatch_rpc(
            &ctx,
            "create_checkpoint",
            json!({
                "sessionId": session_id,
                "turnNumber": 1,
                "label": "initial",
                "cwd": temp_dir.to_string_lossy().to_string(),
            }),
        )
        .await
        .unwrap();
        assert!(cp.is_null() || cp.is_object());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn dispatch_rpc_env_secrets_headless() {
        let temp_dir =
            std::env::temp_dir().join(format!("sakana-remote-env-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let ctx = make_test_context(&temp_dir);
        let key = format!("SAKANA_TEST_SECRET_{}", Uuid::new_v4().simple());

        let set_res = dispatch_rpc(
            &ctx,
            "set_env_secret",
            json!({ "key": key, "value": "super-secret-value" }),
        )
        .await
        .unwrap();
        assert!(set_res.is_array());

        let revealed = dispatch_rpc(&ctx, "reveal_env_secret", json!({ "key": key }))
            .await
            .unwrap();
        assert_eq!(revealed, json!("super-secret-value"));

        let del_res = dispatch_rpc(&ctx, "delete_env_secret", json!({ "key": key }))
            .await
            .unwrap();
        assert!(del_res.is_array());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn dispatch_rpc_quota_accounts_headless() {
        let temp_dir =
            std::env::temp_dir().join(format!("sakana-remote-quota-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let ctx = make_test_context(&temp_dir);

        let created = dispatch_rpc(
            &ctx,
            "upsert_quota_account",
            json!({
                "harness": "claude",
                "provider": "anthropic",
                "label": "Test Quota Account",
                "isDefault": true,
            }),
        )
        .await
        .unwrap();
        let arr = created.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        let id = arr[0]["id"].as_str().unwrap().to_string();

        let after_delete = dispatch_rpc(&ctx, "delete_quota_account", json!({ "id": id }))
            .await
            .unwrap();
        assert!(after_delete.as_array().unwrap().is_empty());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
