# medialab recording (Tauri v2)

Ultra-lightweight Windows screen recorder built with **Tauri v2 (Rust + WebView2) + React + FFmpeg (`gdigrab`)**, targeting Bandicam-like installer size (~30–40 MB) and smooth GPU-accelerated recording.

## Features
- **Bandicam-like Footprint:** Uses Windows WebView2 instead of bundling Chromium (~30–40 MB NSIS installer, ~90–120 MB installed).
- **Zero-Lag GPU Hardware Encoding:** Automatically detects NVIDIA NVENC (`h264_nvenc`), Intel QuickSync (`h264_qsv`), or AMD AMF (`h264_amf`) with fallback to `libx264`.
- **Instant Start & Stop + Cancel/Discard:** Immediate recording start (optional 3s countdown in Settings), fast stream-copy saving, and a Cancel/Discard button right on the floating HUD bar.
- **Built-in Auto-Updater (`tauri-plugin-updater`):** Checks GitHub Releases for new versions and downloads/installs updates with one click from inside Settings or the header banner.
- **Bilingual (EN / AR) with Full RTL:** Complete Arabic and English support across the main window, floating HUD bar, and region picker.

## How to Enable Automatic Updates (One-Time Setup)
1. Generate a Tauri signing key pair once on your machine (or via `npx @tauri-apps/cli signer generate -w ~/.tauri/medialab.key`).
2. Copy the **public key** into `src-tauri/tauri.conf.json` under `plugins.updater.pubkey`, and replace `YOUR_GITHUB_USERNAME/YOUR_REPO_NAME` with your actual GitHub repository.
3. In your GitHub repository → **Settings → Secrets and variables → Actions**, add:
   - `TAURI_SIGNING_PRIVATE_KEY` (contents of the `.key` file)
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (the password you chose, if any)
4. Whenever you want to ship an update: bump `"version"` in `package.json` and `src-tauri/tauri.conf.json` (e.g., `1.1.0` → `1.2.0`) and push to `main`. GitHub Actions will build, sign, and publish `latest.json` + the installer automatically, and existing users will see the update inside the app!

> **Note:** Even before adding the signing keys, pushing to `main` builds the Windows `.exe` installer automatically as a GitHub Actions artifact.
