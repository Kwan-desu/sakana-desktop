use serde::Serialize;
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, Url, WebviewBuilder, WebviewUrl};

pub const BROWSER_WEBVIEW_LABEL: &str = "sakana-browser";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserState {
    pub open: bool,
    pub visible: bool,
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserNavigatedEvent {
    pub url: String,
    pub loading: bool,
}

pub fn normalize_url(raw: &str) -> Result<Url, String> {
    let trimmed = raw.trim();
    let with_scheme = if trimmed.is_empty() {
        "http://localhost:5173".to_string()
    } else if trimmed.eq_ignore_ascii_case("about:blank") {
        "about:blank".to_string()
    } else if trimmed.contains("://") {
        trimmed.to_string()
    } else if trimmed.starts_with("localhost")
        || trimmed.starts_with("127.0.0.1")
        || trimmed.starts_with("[::1]")
        || trimmed
            .split(':')
            .next()
            .map(|h| h.chars().all(|c| c.is_ascii_digit() || c == '.'))
            .unwrap_or(false)
    {
        format!("http://{}", trimmed)
    } else {
        format!("https://{}", trimmed)
    };

    let parsed = Url::parse(&with_scheme).map_err(|e| format!("Invalid URL '{}': {}", raw, e))?;
    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" && parsed.as_str() != "about:blank" {
        return Err(format!("Unsupported URL scheme '{}' (only http/https allowed)", scheme));
    }
    Ok(parsed)
}

fn main_window(app: &AppHandle) -> Result<tauri::Window, String> {
    if let Some(w) = app.get_window("main") {
        return Ok(w);
    }
    app.windows()
        .into_values()
        .next()
        .ok_or_else(|| "Main window not found".to_string())
}

#[tauri::command]
pub async fn browser_open_or_navigate(
    app: AppHandle,
    url: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<BrowserState, String> {
    let parsed = normalize_url(&url)?;
    let pos = LogicalPosition::new(x.max(0.0), y.max(0.0));
    let size = LogicalSize::new(width.max(120.0), height.max(120.0));

    if let Some(wv) = app.get_webview(BROWSER_WEBVIEW_LABEL) {
        let _ = wv.set_position(pos);
        let _ = wv.set_size(size);
        let _ = wv.show();
        wv.navigate(parsed.clone()).map_err(|e| e.to_string())?;
        crate::platform::emit_app_and_remote(
            Some(&app),
            "browser:navigated",
            BrowserNavigatedEvent {
                url: parsed.to_string(),
                loading: false,
            },
        );
        return Ok(BrowserState {
            open: true,
            visible: true,
            url: parsed.to_string(),
        });
    }

    let win = main_window(&app)?;
    let app_for_nav = app.clone();
    let builder = WebviewBuilder::new(BROWSER_WEBVIEW_LABEL, WebviewUrl::External(parsed.clone()))
        .on_navigation(move |nav_url| {
            crate::platform::emit_app_and_remote(
                Some(&app_for_nav),
                "browser:navigated",
                BrowserNavigatedEvent {
                    url: nav_url.to_string(),
                    loading: false,
                },
            );
            true
        });

    win.add_child(builder, pos, size)
        .map_err(|e| format!("Failed to create embedded browser webview: {}", e))?;

    Ok(BrowserState {
        open: true,
        visible: true,
        url: parsed.to_string(),
    })
}

#[tauri::command]
pub async fn browser_set_bounds(
    app: AppHandle,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    if let Some(wv) = app.get_webview(BROWSER_WEBVIEW_LABEL) {
        let _ = wv.set_position(LogicalPosition::new(x.max(0.0), y.max(0.0)));
        let _ = wv.set_size(LogicalSize::new(width.max(1.0), height.max(1.0)));
    }
    Ok(())
}

#[tauri::command]
pub async fn browser_set_visible(app: AppHandle, visible: bool) -> Result<(), String> {
    if let Some(wv) = app.get_webview(BROWSER_WEBVIEW_LABEL) {
        if visible {
            wv.show().map_err(|e| e.to_string())?;
        } else {
            wv.hide().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn browser_nav_action(app: AppHandle, action: String) -> Result<(), String> {
    let Some(wv) = app.get_webview(BROWSER_WEBVIEW_LABEL) else {
        return Ok(());
    };
    match action.as_str() {
        "back" => wv.eval("window.history.back();").map_err(|e| e.to_string())?,
        "forward" => wv.eval("window.history.forward();").map_err(|e| e.to_string())?,
        "reload" => wv.eval("window.location.reload();").map_err(|e| e.to_string())?,
        _ => return Err(format!("Unknown browser action: {}", action)),
    }
    Ok(())
}

#[tauri::command]
pub async fn browser_eval_js(app: AppHandle, script: String) -> Result<(), String> {
    let Some(wv) = app.get_webview(BROWSER_WEBVIEW_LABEL) else {
        return Err("Embedded browser is not open".to_string());
    };
    wv.eval(&script).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn browser_get_state(app: AppHandle) -> Result<BrowserState, String> {
    if let Some(wv) = app.get_webview(BROWSER_WEBVIEW_LABEL) {
        let url = wv.url().map(|u| u.to_string()).unwrap_or_default();
        return Ok(BrowserState {
            open: true,
            visible: true,
            url,
        });
    }
    Ok(BrowserState {
        open: false,
        visible: false,
        url: String::new(),
    })
}

#[tauri::command]
pub async fn browser_close(app: AppHandle) -> Result<(), String> {
    if let Some(wv) = app.get_webview(BROWSER_WEBVIEW_LABEL) {
        let _ = wv.close();
    }
    Ok(())
}
