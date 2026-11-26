# Sakana Desktop

Cross-platform **Windows & Linux** desktop application for Sakana — an agentic AI control surface unifying **AGY CLI**, **Claude Code**, **Kiro CLI**, and **Sakana Master** with a native ConPTY/POSIX terminal, Git diff/hunk/worktree inspector, 4-tier Terminal Guard, MCP Server Hub, cron/webhook automations, Discord bot bridge, and LAN remote access.

## Stack

- **Shell & Native Core:** Rust + [Tauri v2](https://v2.tauri.app/)
- **UI:** React 19 + TypeScript + Tailwind CSS v4 + xterm.js + CodeMirror 6
- **Database:** Bundled SQLite (`rusqlite`, WAL mode) stored in OS app data dir
- **PTY:** `portable-pty` (Windows ConPTY / Linux POSIX PTY)
- **LAN Remote Gateway:** Embedded `axum` + `tower-http` serving built `dist/` + Bearer/token auth + `/hooks/{path}` webhooks + QR code
- **System Integration:** System tray (close-to-tray + click toggle), `tauri-plugin-autostart`, `tauri-plugin-updater`

## Prerequisites

- **Node.js 20+** and **Rust (stable)**
- **Windows:** Visual Studio Build Tools (MSVC) + WebView2 (preinstalled on Windows 10/11)
- **Linux (Ubuntu/Debian):**
  ```bash
  sudo apt-get install -y libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf libssl-dev
  ```

## Development

```bash
npm install
npm run tauri dev
```

## Production Build

```bash
npm run tauri build
```

Artifacts are generated under `src-tauri/target/release/bundle/`:
- **Windows:** `.exe` (NSIS) and `.msi`
- **Linux:** `.AppImage` and `.deb`

## Data & Secrets Location

Runtime data (`sakana.db` and `.env` secrets) is stored in the OS app data directory rather than the install folder:
- **Windows:** `%APPDATA%\ai.sakana.desktop\`
- **Linux:** `~/.local/share/ai.sakana.desktop/`

## Auto-Updater & CI Releases

1. Generate a signing keypair once:
   ```bash
   npx tauri signer generate -w ~/.tauri/sakana.key
   ```
2. Paste the public key into `src-tauri/tauri.conf.json` under `plugins.updater.pubkey`.
3. Add `TAURI_SIGNING_PRIVATE_KEY` (and optional `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`) to GitHub Actions Secrets.
4. Push a tag (`git tag v2.0.0 && git push origin v2.0.0`) to trigger `.github/workflows/release.yml`.
