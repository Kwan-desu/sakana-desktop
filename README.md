# Sakana Desktop

Cross-platform **Windows & Linux** desktop application for Sakana — an agentic AI control surface unifying **AGY CLI**, **Claude Code**, **Kiro CLI**, and **Sakana Master** with a native ConPTY/POSIX terminal, Git diff/hunk/worktree inspector, 4-tier Terminal Guard, MCP Server Hub, cron/webhook automations, Discord bot bridge, and LAN remote access.

## Stack

- **Shell & Native Core:** Rust + [Tauri v2](https://v2.tauri.app/)
- **UI:** React 19 + TypeScript + Tailwind CSS v4 + xterm.js + CodeMirror 6
- **Database:** Bundled SQLite (`rusqlite`, WAL mode) stored in OS app data dir
- **PTY:** `portable-pty` (Windows ConPTY / Linux POSIX PTY)
- **LAN Remote Gateway:** Embedded `axum` + `tower-http` serving built `dist/` + Bearer/token auth + `/hooks/{path}` webhooks + QR code
- **System Integration:** System tray (close-to-tray + click toggle), `tauri-plugin-autostart`, `tauri-plugin-updater`

## Installing on Linux

Download the latest release from the [GitHub Releases](https://github.com/Kwan-desu/sakana-desktop/releases/latest) page.

### Option 1: Debian / Ubuntu (`.deb` — Recommended)

```bash
# Download the latest .deb package from GitHub Releases, then install:
sudo apt update
sudo apt install -y ./Sakana_*_amd64.deb
```

Launch **Sakana** from your application menu or run `sakana` in a terminal.

> **Note:** On Ubuntu 24.04+ / Debian 12+, `apt install ./Sakana_*_amd64.deb` automatically installs required runtime dependencies (`libwebkit2gtk-4.1-0`, `libayatana-appindicator3-1`).

### Option 2: Universal Linux (`.AppImage`)

```bash
# Make the AppImage executable and run it:
chmod +x Sakana_*_amd64.AppImage
./Sakana_*_amd64.AppImage
```

- On Ubuntu 22.04+ if AppImage reports missing FUSE, install FUSE 2:
  ```bash
  sudo apt install -y libfuse2
  ```
- Alternatively, run without FUSE:
  ```bash
  ./Sakana_*_amd64.AppImage --appimage-extract-and-run
  ```

### Option 3: Build from Source on Linux

1. Install system dependencies (Ubuntu/Debian):
   ```bash
   sudo apt-get update
   sudo apt-get install -y \
     build-essential curl wget file libssl-dev \
     libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf
   ```
2. Install Node.js 20+ and Rust (via `rustup`), then clone and build:
   ```bash
   git clone https://github.com/Kwan-desu/sakana-desktop.git
   cd sakana-desktop
   npm install
   npm run tauri build
   ```
3. Built `.deb` and `.AppImage` bundles will be in `src-tauri/target/release/bundle/`.

## Prerequisites (Development)

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
