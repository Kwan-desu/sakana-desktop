use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

use sakana_lib::db::Database;
use sakana_lib::platform;
use sakana_lib::remote;

#[tokio::main]
async fn main() {
    let mut port: u16 = std::env::var("SAKANA_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4317);
    let mut cli_token: Option<String> = None;
    let mut lan_enabled = std::env::var("SAKANA_REMOTE_LAN")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--port" | "-p" => {
                if let Some(v) = args.get(i + 1).and_then(|s| s.parse().ok()) {
                    port = v;
                    i += 1;
                }
            }
            "--token" | "-t" => {
                if let Some(v) = args.get(i + 1) {
                    if !v.trim().is_empty() {
                        cli_token = Some(v.trim().to_string());
                    }
                    i += 1;
                }
            }
            "--lan" | "--tailscale" => {
                lan_enabled = true;
            }
            _ => {}
        }
        i += 1;
    }

    if lan_enabled {
        std::env::set_var("SAKANA_REMOTE_LAN", "1");
    }
    if let Some(ref t) = cli_token {
        std::env::set_var("SAKANA_TOKEN", t);
    }

    let data_dir = platform::app_data_dir();
    platform::init_app_data_dir(data_dir.clone());
    let db = Database::open_in_dir(&data_dir).expect("Failed to open Sakana SQLite database");
    let fallback_token = cli_token
        .unwrap_or_else(|| Uuid::new_v4().to_string().replace('-', ""));
    let token = remote::load_or_init_remote_token(&db, &fallback_token);
    let db_arc = Arc::new(Mutex::new(db));

    let bind_host = if lan_enabled { "0.0.0.0" } else { "127.0.0.1" };
    let (ts_ip, ts_dns) = remote::detect_tailscale_info();
    let ip = if lan_enabled {
        ts_ip.clone().unwrap_or_else(remote::detect_lan_ip)
    } else {
        "127.0.0.1".to_string()
    };
    let pairing = remote::build_pairing_link(&ip, port, &token);

    println!("Sakana Headless Daemon (sakana-rpc-v2)");
    println!("  Data dir : {}", data_dir.display());
    println!("  Listening: http://{}:{}", bind_host, port);
    if let Some(ref tip) = ts_ip {
        println!("  Tailscale: {} (MagicDNS: {})", tip, ts_dns.as_deref().unwrap_or("n/a"));
    }
    println!("  Pairing  : {}", pairing);
    println!("Paste the Pairing URI into Sakana Desktop -> Dashboard -> Remote Access -> Link Instance.");

    if let Err(err) = remote::serve_remote_daemon(db_arc, port, token).await {
        eprintln!("sakana-daemon exited with error: {}", err);
        std::process::exit(1);
    }
}
