use chrono::Local;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, Size, State, WebviewUrl, WebviewWindowBuilder,
    WindowEvent,
};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

fn hidden_cmd(program: &str) -> Command {
    let mut cmd = Command::new(program);
    #[cfg(target_os = "windows")]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionCoordinates {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionResult {
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub cancelled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordOptions {
    pub mode: String,
    pub region: Option<RegionCoordinates>,
    pub window_title: Option<String>,
    pub resolution: String,
    pub fps: u32,
    pub sharpness: String,
    pub hide_cursor: bool,
    #[serde(default)]
    pub system_audio: bool,
    #[serde(default)]
    pub mic_audio: bool,
    pub format: String,
    pub save_path: Option<String>,
    pub current_index: Option<u32>,
    pub prefix: Option<String>,
    pub auto_sequential: Option<bool>,
    pub lang: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowInfo {
    pub id: String,
    pub title: String,
    pub app_name: String,
    pub icon: String,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveResult {
    pub path: String,
    pub file_name: String,
    pub duration_seconds: u64,
    pub current_index: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HudStatePayload {
    pub paused: bool,
    pub elapsed_ms: u64,
    pub lang: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayStatePayload {
    pub image: Option<String>,
    pub origin_x: i32,
    pub origin_y: i32,
    pub lang: String,
}

struct AudioCaptureHandle {
    stop_tx: mpsc::Sender<()>,
    join_handle: std::thread::JoinHandle<()>,
    wav_path: PathBuf,
}

fn start_audio_capture(is_system_loopback: bool, wav_path: PathBuf) -> Option<AudioCaptureHandle> {
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let (ready_tx, ready_rx) = mpsc::channel::<bool>();
    let path_clone = wav_path.clone();

    let join_handle = std::thread::spawn(move || {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
        let host = cpal::default_host();
        let (device, config) = if is_system_loopback {
            let Some(dev) = host.default_output_device() else {
                let _ = ready_tx.send(false);
                return;
            };
            let Ok(cfg) = dev.default_output_config() else {
                let _ = ready_tx.send(false);
                return;
            };
            (dev, cfg)
        } else {
            let Some(dev) = host.default_input_device() else {
                let _ = ready_tx.send(false);
                return;
            };
            let Ok(cfg) = dev.default_input_config() else {
                let _ = ready_tx.send(false);
                return;
            };
            (dev, cfg)
        };

        let spec = hound::WavSpec {
            channels: config.channels(),
            sample_rate: config.sample_rate().0,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let Ok(wav_writer) = hound::WavWriter::create(&path_clone, spec) else {
            let _ = ready_tx.send(false);
            return;
        };
        let writer = Arc::new(Mutex::new(Some(wav_writer)));
        let sample_format = config.sample_format();
        let stream_config: cpal::StreamConfig = config.into();
        let w_cb = writer.clone();

        let stream_res = match sample_format {
            cpal::SampleFormat::F32 => device.build_input_stream(
                &stream_config,
                move |data: &[f32], _| {
                    if let Ok(mut guard) = w_cb.lock() {
                        if let Some(ref mut w) = *guard {
                            for &s in data {
                                let clamped = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
                                let _ = w.write_sample(clamped);
                            }
                        }
                    }
                },
                |_| {},
                None,
            ),
            cpal::SampleFormat::I16 => device.build_input_stream(
                &stream_config,
                move |data: &[i16], _| {
                    if let Ok(mut guard) = w_cb.lock() {
                        if let Some(ref mut w) = *guard {
                            for &s in data {
                                let _ = w.write_sample(s);
                            }
                        }
                    }
                },
                |_| {},
                None,
            ),
            cpal::SampleFormat::U16 => device.build_input_stream(
                &stream_config,
                move |data: &[u16], _| {
                    if let Ok(mut guard) = w_cb.lock() {
                        if let Some(ref mut w) = *guard {
                            for &s in data {
                                let centered = (s as i32 - 32768) as i16;
                                let _ = w.write_sample(centered);
                            }
                        }
                    }
                },
                |_| {},
                None,
            ),
            _ => {
                let _ = ready_tx.send(false);
                return;
            }
        };

        let Ok(stream) = stream_res else {
            let _ = ready_tx.send(false);
            return;
        };
        if stream.play().is_err() {
            let _ = ready_tx.send(false);
            return;
        }
        let _ = ready_tx.send(true);
        let _ = stop_rx.recv();
        drop(stream);
        if let Ok(mut guard) = writer.lock() {
            if let Some(w) = guard.take() {
                let _ = w.finalize();
            }
        };
    });

    if ready_rx.recv_timeout(Duration::from_secs(2)).unwrap_or(false) {
        Some(AudioCaptureHandle {
            stop_tx,
            join_handle,
            wav_path,
        })
    } else {
        let _ = stop_tx.send(());
        let _ = join_handle.join();
        None
    }
}

fn stop_audio_handle(handle: Option<AudioCaptureHandle>) -> Option<PathBuf> {
    let h = handle?;
    let _ = h.stop_tx.send(());
    let _ = h.join_handle.join();
    if fs::metadata(&h.wav_path).map(|m| m.len() > 128).unwrap_or(false) {
        Some(h.wav_path)
    } else {
        None
    }
}

struct RecorderInner {
    active: bool,
    paused: bool,
    stopping: bool,
    ffmpeg: Option<Child>,
    sys_audio_handle: Option<AudioCaptureHandle>,
    mic_audio_handle: Option<AudioCaptureHandle>,
    segments: Vec<PathBuf>,
    sys_audio_segments: Vec<PathBuf>,
    mic_audio_segments: Vec<PathBuf>,
    segment_index: usize,
    options: Option<RecordOptions>,
    started_at: Option<Instant>,
    elapsed_ms: u64,
    detected_encoder: Option<String>,
    windows_cache: (Option<Instant>, Vec<WindowInfo>),
    region_tx: Option<mpsc::Sender<RegionResult>>,
    overlay_payload: Option<OverlayStatePayload>,
    temp_root: PathBuf,
}

pub struct AppState {
    inner: Arc<Mutex<RecorderInner>>,
}

impl AppState {
    fn new() -> Self {
        let temp_root = std::env::temp_dir().join(format!("medialab-recording-{}", std::process::id()));
        Self {
            inner: Arc::new(Mutex::new(RecorderInner {
                active: false,
                paused: false,
                stopping: false,
                ffmpeg: None,
                sys_audio_handle: None,
                mic_audio_handle: None,
                segments: Vec::new(),
                sys_audio_segments: Vec::new(),
                mic_audio_segments: Vec::new(),
                segment_index: 0,
                options: None,
                started_at: None,
                elapsed_ms: 0,
                detected_encoder: None,
                windows_cache: (None, Vec::new()),
                region_tx: None,
                overlay_payload: None,
                temp_root,
            })),
        }
    }
}

fn preferences_path(app: &AppHandle) -> PathBuf {
    let dir = app.path().app_data_dir().unwrap_or_else(|_| std::env::temp_dir());
    dir.join("medialab-preferences.json")
}

fn log_line(app: &AppHandle, msg: &str) {
    if let Ok(dir) = app.path().app_data_dir() {
        let _ = fs::create_dir_all(&dir);
        let log_file = dir.join("medialab.log");
        if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(log_file) {
            let _ = writeln!(f, "{} {}", Local::now().to_rfc3339(), msg);
        }
    }
}

fn read_preferences(app: &AppHandle) -> Value {
    let path = preferences_path(app);
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| json!({}))
}

fn write_preferences(app: &AppHandle, patch: &Value) -> Value {
    let mut current = read_preferences(app);
    if let (Some(curr_obj), Some(patch_obj)) = (current.as_object_mut(), patch.as_object()) {
        for (k, v) in patch_obj {
            curr_obj.insert(k.clone(), v.clone());
        }
    }
    let path = preferences_path(app);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let tmp = path.with_extension("json.tmp");
    if let Ok(text) = serde_json::to_string_pretty(&current) {
        if fs::write(&tmp, text).is_ok() {
            let _ = fs::rename(&tmp, &path);
        }
    }
    current
}

fn clean_stale_temp_folders(current_temp: &Path) {
    let temp_dir = std::env::temp_dir();
    let week = Duration::from_secs(7 * 24 * 3600);
    if let Ok(entries) = fs::read_dir(&temp_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with("medialab-recording-") {
                continue;
            }
            let path = entry.path();
            if path == current_temp {
                continue;
            }
            if let Ok(meta) = entry.metadata() {
                if let Ok(modified) = meta.modified() {
                    if let Ok(age) = SystemTime::now().duration_since(modified) {
                        if age > week {
                            let _ = fs::remove_dir_all(&path);
                        }
                    }
                }
            }
        }
    }
}

fn resolve_ffmpeg(app: &AppHandle) -> String {
    if let Ok(res_dir) = app.path().resource_dir() {
        let candidate = res_dir.join("ffmpeg.exe");
        if candidate.exists() {
            return candidate.to_string_lossy().to_string();
        }
    }
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(dir) = exe_path.parent() {
            let c1 = dir.join("ffmpeg.exe");
            if c1.exists() {
                return c1.to_string_lossy().to_string();
            }
            let c2 = dir.join("resources").join("ffmpeg.exe");
            if c2.exists() {
                return c2.to_string_lossy().to_string();
            }
        }
    }
    let dev_path = PathBuf::from("src-tauri").join("bin").join("ffmpeg.exe");
    if dev_path.exists() {
        return dev_path.to_string_lossy().to_string();
    }
    if cfg!(target_os = "windows") {
        "ffmpeg.exe".to_string()
    } else {
        "ffmpeg".to_string()
    }
}

fn probe_hardware_encoder_sync(exe: &str, app: &AppHandle, state: &Arc<Mutex<RecorderInner>>) -> String {
    if let Ok(guard) = state.lock() {
        if let Some(ref enc) = guard.detected_encoder {
            return enc.clone();
        }
    }
    let prefs = read_preferences(app);
    if let Some(pref_enc) = prefs.get("videoEncoder").and_then(|v| v.as_str()) {
        if matches!(pref_enc, "h264_nvenc" | "h264_qsv" | "h264_amf" | "libx264") {
            if let Ok(mut guard) = state.lock() {
                guard.detected_encoder = Some(pref_enc.to_string());
            }
            return pref_enc.to_string();
        }
    }

    for codec in ["h264_nvenc", "h264_qsv", "h264_amf"] {
        let ok = hidden_cmd(exe)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=c=black:s=256x256:d=0.05",
                "-frames:v",
                "1",
                "-c:v",
                codec,
                "-f",
                "null",
                "-",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            log_line(app, &format!("Hardware encoder detected: {}", codec));
            if let Ok(mut guard) = state.lock() {
                guard.detected_encoder = Some(codec.to_string());
            }
            return codec.to_string();
        }
    }

    log_line(app, "Using software encoder: libx264");
    if let Ok(mut guard) = state.lock() {
        guard.detected_encoder = Some("libx264".to_string());
    }
    "libx264".to_string()
}

fn even_floor(n: i32) -> i32 {
    let v = (n / 2) * 2;
    if v < 2 { 2 } else { v }
}

fn next_free_index(folder: &Path, prefix: &str) -> u32 {
    let clean = prefix.trim();
    let mut idx: u32 = 0;
    loop {
        let exists = ["mp4", "mkv", "mov"]
            .iter()
            .any(|ext| folder.join(format!("{}{}.{}", clean, idx, ext)).exists());
        if !exists {
            return idx;
        }
        idx += 1;
    }
}

fn choose_output_path(folder: &Path, ext: &str, opts: &RecordOptions) -> (PathBuf, u32) {
    let start_idx = opts.current_index.unwrap_or(0);
    if opts.auto_sequential == Some(false) {
        let base = format!("recording_{}", Local::now().format("%Y-%m-%d"));
        let mut out = folder.join(format!("{}.{}", base, ext));
        let mut n = 2;
        while out.exists() {
            out = folder.join(format!("{}_{}.{}", base, n, ext));
            n += 1;
        }
        return (out, start_idx);
    }
    let clean = opts.prefix.as_deref().unwrap_or("").trim();
    let mut idx = start_idx;
    loop {
        let out = folder.join(format!("{}{}.{}", clean, idx, ext));
        idx += 1;
        if !out.exists() {
            return (out, idx);
        }
    }
}

fn build_video_filter(resolution: &str) -> String {
    let target = match resolution {
        "4k" => Some((3840, 2160)),
        "1080p" => Some((1920, 1080)),
        "720p" => Some((1280, 720)),
        _ => None,
    };
    let Some((long, short)) = target else {
        return "scale=trunc(iw/2)*2:trunc(ih/2)*2".to_string();
    };
    let w = format!("'if(gte(iw,ih),{},{})'", long, short);
    let h = format!("'if(gte(iw,ih),{},{})'", short, long);
    format!(
        "scale={}:{}:force_original_aspect_ratio=decrease:force_divisible_by=2:flags=lanczos,pad={}:{}:(ow-iw)/2:(oh-ih)/2:color=black",
        w, h, w, h
    )
}

fn build_encoder_args(codec: &str, opts: &RecordOptions, app: &AppHandle) -> Vec<String> {
    let fps = match opts.fps {
        24 | 30 | 60 => opts.fps,
        _ => 30,
    };
    let gop = (fps * 2).to_string();
    let q = match opts.sharpness.as_str() {
        "ultra" => "16",
        "high" => "19",
        _ => "22",
    };

    match codec {
        "h264_nvenc" => vec![
            "-c:v".into(), "h264_nvenc".into(),
            "-preset".into(), "p4".into(),
            "-tune".into(), "ll".into(),
            "-rc".into(), "vbr".into(),
            "-cq".into(), q.into(),
            "-b:v".into(), "0".into(),
            "-g".into(), gop,
            "-pix_fmt".into(), "yuv420p".into(),
        ],
        "h264_qsv" => vec![
            "-c:v".into(), "h264_qsv".into(),
            "-preset".into(), "veryfast".into(),
            "-global_quality".into(), q.into(),
            "-g".into(), gop,
            "-pix_fmt".into(), "nv12".into(),
        ],
        "h264_amf" => vec![
            "-c:v".into(), "h264_amf".into(),
            "-quality".into(), "speed".into(),
            "-rc".into(), "cqp".into(),
            "-qp_i".into(), q.into(),
            "-qp_p".into(), q.into(),
            "-g".into(), gop,
            "-pix_fmt".into(), "yuv420p".into(),
        ],
        _ => {
            let prefs = read_preferences(app);
            let preset = prefs
                .get("encoderPreset")
                .and_then(|v| v.as_str())
                .filter(|p| matches!(*p, "ultrafast" | "superfast" | "veryfast" | "faster" | "fast"))
                .unwrap_or("ultrafast");
            let mut v = vec!["-c:v".into(), "libx264".into(), "-preset".into(), preset.into()];
            if preset == "ultrafast" {
                v.push("-tune".into());
                v.push("zerolatency".into());
            }
            v.extend([
                "-crf".into(), q.into(),
                "-g".into(), gop,
                "-pix_fmt".into(), "yuv420p".into(),
            ]);
            v
        }
    }
}

fn stop_child_gracefully(mut child: Child) {
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(b"q\n");
        let _ = stdin.flush();
    }
    let deadline = Instant::now() + Duration::from_secs(6);
    while Instant::now() < deadline {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) => std::thread::sleep(Duration::from_millis(35)),
            Err(_) => break,
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn kill_child_now(mut child: Child) {
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(b"q\n");
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn close_window(app: &AppHandle, label: &str) {
    if let Some(win) = app.get_webview_window(label) {
        let _ = win.destroy();
    }
}

fn show_hud_window(app: &AppHandle) {
    close_window(app, "hud");
    let (x, y) = if let Ok(Some(monitor)) = app.primary_monitor() {
        let scale = monitor.scale_factor();
        let lw = monitor.size().width as f64 / scale;
        let lx = monitor.position().x as f64 / scale;
        let ly = monitor.position().y as f64 / scale;
        (lx + (lw - 520.0) / 2.0, ly + 14.0)
    } else {
        (400.0, 14.0)
    };

    if let Ok(win) = WebviewWindowBuilder::new(app, "hud", WebviewUrl::App("hud.html".into()))
        .title("Recording controls")
        .inner_size(520.0, 56.0)
        .position(x, y)
        .decorations(false)
        .transparent(true)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .shadow(false)
        .build()
    {
        let _ = win.set_content_protected(true);
    }
}

fn start_segment_internal(app: &AppHandle, state: &Arc<Mutex<RecorderInner>>) -> Result<(), String> {
    let (opts, temp_root, seg_idx) = {
        let mut guard = state.lock().map_err(|_| "State lock poisoned".to_string())?;
        let opts = guard.options.clone().ok_or("Missing recording options")?;
        let idx = guard.segment_index;
        guard.segment_index += 1;
        (opts, guard.temp_root.clone(), idx)
    };

    fs::create_dir_all(&temp_root).map_err(|e| e.to_string())?;
    let seg_path = temp_root.join(format!("segment-{}.mkv", seg_idx));
    let exe = resolve_ffmpeg(app);
    let codec = probe_hardware_encoder_sync(&exe, app, state);
    let fps = match opts.fps {
        24 | 30 | 60 => opts.fps,
        _ => 30,
    };

    let build_full_args = |active_codec: &str| -> Vec<String> {
        let mut args: Vec<String> = vec![
            "-hide_banner".into(),
            "-loglevel".into(), "warning".into(),
            "-rtbufsize".into(), "256M".into(),
            "-probesize".into(), "10M".into(),
            "-thread_queue_size".into(), "1024".into(),
            "-f".into(), "gdigrab".into(),
            "-framerate".into(), fps.to_string(),
            "-draw_mouse".into(), if opts.hide_cursor { "0" } else { "1" }.into(),
        ];

        let primary = app.primary_monitor().ok().flatten();
        let scale = primary.as_ref().map(|m| m.scale_factor()).unwrap_or(1.0);
        let mon_x = primary.as_ref().map(|m| m.position().x).unwrap_or(0);
        let mon_y = primary.as_ref().map(|m| m.position().y).unwrap_or(0);
        let mon_w = primary.as_ref().map(|m| m.size().width as i32).unwrap_or(1920);
        let mon_h = primary.as_ref().map(|m| m.size().height as i32).unwrap_or(1080);

        if opts.mode == "region" {
            if let Some(ref r) = opts.region {
                let px = ((r.x as f64) * scale).round() as i32;
                let py = ((r.y as f64) * scale).round() as i32;
                let pw = even_floor(((r.width as f64) * scale).round() as i32);
                let ph = even_floor(((r.height as f64) * scale).round() as i32);
                let cx = px.max(mon_x).min(mon_x + mon_w - 2);
                let cy = py.max(mon_y).min(mon_y + mon_h - 2);
                let cw = even_floor(pw.min(mon_x + mon_w - cx));
                let ch = even_floor(ph.min(mon_y + mon_h - cy));
                args.extend([
                    "-offset_x".into(), cx.max(0).to_string(),
                    "-offset_y".into(), cy.max(0).to_string(),
                    "-video_size".into(), format!("{}x{}", cw, ch),
                ]);
            }
        } else if opts.mode == "fullscreen" {
            let monitor_count = app.available_monitors().map(|v| v.len()).unwrap_or(1);
            if monitor_count > 1 {
                args.extend([
                    "-offset_x".into(), mon_x.max(0).to_string(),
                    "-offset_y".into(), mon_y.max(0).to_string(),
                    "-video_size".into(), format!("{}x{}", even_floor(mon_w), even_floor(mon_h)),
                ]);
            }
        }

        if opts.mode == "window" {
            if let Some(ref title) = opts.window_title {
                if !title.is_empty() {
                    args.extend(["-i".into(), format!("title={}", title)]);
                } else {
                    args.extend(["-i".into(), "desktop".into()]);
                }
            } else {
                args.extend(["-i".into(), "desktop".into()]);
            }
        } else {
            args.extend(["-i".into(), "desktop".into()]);
        }

        args.extend([
            "-vf".into(), build_video_filter(&opts.resolution),
            "-fps_mode".into(), "cfr".into(),
            "-r".into(), fps.to_string(),
        ]);
        args.extend(build_encoder_args(active_codec, &opts, app));
        args.extend(["-f".into(), "matroska".into(), "-y".into(), seg_path.to_string_lossy().to_string()]);
        args
    };

    let spawn_with = |active_codec: &str| -> Result<Child, String> {
        let args = build_full_args(active_codec);
        let mut child = hidden_cmd(&exe)
            .args(&args)
            .env("__COMPAT_LAYER", "HIGHDPIAWARE")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to start FFmpeg: {}", e))?;

        for _ in 0..6 {
            std::thread::sleep(Duration::from_millis(50));
            if let Ok(Some(status)) = child.try_wait() {
                let mut err_msg = String::new();
                if let Some(mut stderr) = child.stderr.take() {
                    let _ = stderr.read_to_string(&mut err_msg);
                }
                let trimmed = err_msg.trim();
                let short_err = if trimmed.len() > 260 {
                    &trimmed[trimmed.len() - 260..]
                } else {
                    trimmed
                };
                return Err(format!("FFmpeg failed ({}) [{}]: {}", status, active_codec, short_err));
            }
        }
        Ok(child)
    };

    let child = match spawn_with(&codec) {
        Ok(c) => c,
        Err(err) => {
            if codec != "libx264" {
                log_line(app, &format!("Hardware encoder {} failed at runtime, falling back to libx264: {}", codec, err));
                if let Ok(mut guard) = state.lock() {
                    guard.detected_encoder = Some("libx264".to_string());
                }
                spawn_with("libx264")?
            } else {
                return Err(err);
            }
        }
    };

    // Start native Windows WASAPI system audio / microphone capture alongside FFmpeg
    let sys_handle = if opts.system_audio {
        let wav = temp_root.join(format!("segment-{}-sys.wav", seg_idx));
        let h = start_audio_capture(true, wav);
        if h.is_none() {
            log_line(app, "System audio capture could not be opened; continuing without system audio.");
        }
        h
    } else {
        None
    };

    let mic_handle = if opts.mic_audio {
        let wav = temp_root.join(format!("segment-{}-mic.wav", seg_idx));
        let h = start_audio_capture(false, wav);
        if h.is_none() {
            log_line(app, "Microphone capture could not be opened; continuing without microphone.");
        }
        h
    } else {
        None
    };

    let mut guard = state.lock().map_err(|_| "State lock poisoned".to_string())?;
    guard.ffmpeg = Some(child);
    guard.sys_audio_handle = sys_handle;
    guard.mic_audio_handle = mic_handle;
    guard.segments.push(seg_path);
    Ok(())
}

fn pause_internal(app: &AppHandle, state: &Arc<Mutex<RecorderInner>>) -> Result<(), String> {
    let (child_opt, sys_h, mic_h) = {
        let mut guard = state.lock().map_err(|_| "State lock poisoned".to_string())?;
        if !guard.active || guard.paused || guard.stopping {
            return Ok(());
        }
        if let Some(started) = guard.started_at.take() {
            guard.elapsed_ms += started.elapsed().as_millis() as u64;
        }
        guard.paused = true;
        (guard.ffmpeg.take(), guard.sys_audio_handle.take(), guard.mic_audio_handle.take())
    };

    if let Some(child) = child_opt {
        stop_child_gracefully(child);
    }
    let sys_wav = stop_audio_handle(sys_h);
    let mic_wav = stop_audio_handle(mic_h);

    let elapsed_ms = if let Ok(mut guard) = state.lock() {
        if let Some(p) = sys_wav { guard.sys_audio_segments.push(p); }
        if let Some(p) = mic_wav { guard.mic_audio_segments.push(p); }
        guard.elapsed_ms
    } else {
        0
    };

    let _ = app.emit("recording-status-change", json!({ "status": "paused" }));
    let _ = app.emit("hud-status", json!({ "paused": true, "elapsedMs": elapsed_ms }));
    Ok(())
}

fn resume_internal(app: &AppHandle, state: &Arc<Mutex<RecorderInner>>) -> Result<(), String> {
    {
        let mut guard = state.lock().map_err(|_| "State lock poisoned".to_string())?;
        if !guard.active || !guard.paused || guard.stopping {
            return Ok(());
        }
        guard.started_at = Some(Instant::now());
    }

    if let Err(e) = start_segment_internal(app, state) {
        let elapsed_ms = state.lock().map(|g| g.elapsed_ms).unwrap_or(0);
        let _ = app.emit("recording-error", e.clone());
        let _ = app.emit("recording-status-change", json!({ "status": "paused" }));
        let _ = app.emit("hud-status", json!({ "paused": true, "elapsedMs": elapsed_ms }));
        return Err(e);
    }

    let elapsed_ms = {
        let mut guard = state.lock().map_err(|_| "State lock poisoned".to_string())?;
        guard.paused = false;
        guard.elapsed_ms
    };
    let _ = app.emit("recording-status-change", json!({ "status": "recording" }));
    let _ = app.emit("hud-status", json!({ "paused": false, "elapsedMs": elapsed_ms }));
    Ok(())
}

fn cancel_internal(app: &AppHandle, state: &Arc<Mutex<RecorderInner>>) -> Result<(), String> {
    let (child_opt, sys_h, mic_h, temp_root) = {
        let mut guard = state.lock().map_err(|_| "State lock poisoned".to_string())?;
        if !guard.active || guard.stopping {
            return Ok(());
        }
        guard.stopping = true;
        guard.active = false;
        guard.paused = false;
        guard.segments.clear();
        guard.sys_audio_segments.clear();
        guard.mic_audio_segments.clear();
        guard.segment_index = 0;
        guard.elapsed_ms = 0;
        guard.started_at = None;
        (
            guard.ffmpeg.take(),
            guard.sys_audio_handle.take(),
            guard.mic_audio_handle.take(),
            guard.temp_root.clone(),
        )
    };

    close_window(app, "hud");
    if let Some(child) = child_opt {
        kill_child_now(child);
    }
    let _ = stop_audio_handle(sys_h);
    let _ = stop_audio_handle(mic_h);
    let _ = fs::remove_dir_all(&temp_root);

    if let Ok(mut guard) = state.lock() {
        guard.stopping = false;
    }

    if let Some(main_win) = app.get_webview_window("main") {
        let _ = main_win.unminimize();
        let _ = main_win.show();
        let _ = main_win.set_focus();
    }
    let _ = app.emit("recording-cancelled", json!({}));
    let _ = app.emit("recording-status-change", json!({ "status": "idle" }));
    Ok(())
}

fn write_concat_file(list_path: &Path, items: &[PathBuf]) -> Result<(), String> {
    let list_content: Vec<String> = items
        .iter()
        .map(|p| {
            let escaped = p.to_string_lossy().replace('\\', "/").replace('\'', "'\\''");
            format!("file '{}'", escaped)
        })
        .collect();
    fs::write(list_path, list_content.join("\n")).map_err(|e| e.to_string())
}

fn stop_save_internal(app: &AppHandle, state: &Arc<Mutex<RecorderInner>>) -> Result<Option<SaveResult>, String> {
    let (child_opt, sys_h, mic_h, opts, segments, mut sys_segs, mut mic_segs, temp_root, elapsed_ms) = {
        let mut guard = state.lock().map_err(|_| "State lock poisoned".to_string())?;
        if !guard.active || guard.stopping {
            return Ok(None);
        }
        guard.stopping = true;
        if !guard.paused {
            if let Some(started) = guard.started_at.take() {
                guard.elapsed_ms += started.elapsed().as_millis() as u64;
            }
        }
        let c = guard.ffmpeg.take();
        let sh = guard.sys_audio_handle.take();
        let mh = guard.mic_audio_handle.take();
        let o = guard.options.clone();
        let s = guard.segments.clone();
        let ss = guard.sys_audio_segments.clone();
        let ms = guard.mic_audio_segments.clone();
        let t = guard.temp_root.clone();
        let e = guard.elapsed_ms;
        guard.active = false;
        guard.paused = false;
        (c, sh, mh, o, s, ss, ms, t, e)
    };

    close_window(app, "hud");
    if let Some(child) = child_opt {
        stop_child_gracefully(child);
    }
    if let Some(p) = stop_audio_handle(sys_h) { sys_segs.push(p); }
    if let Some(p) = stop_audio_handle(mic_h) { mic_segs.push(p); }

    if let Some(main_win) = app.get_webview_window("main") {
        let _ = main_win.unminimize();
        let _ = main_win.show();
        let _ = main_win.set_focus();
    }

    let finalize = (|| -> Result<SaveResult, String> {
        let opts = opts.ok_or("Missing recording options")?;
        let valid: Vec<PathBuf> = segments
            .into_iter()
            .filter(|p| fs::metadata(p).map(|m| m.len() > 2048).unwrap_or(false))
            .collect();
        if valid.is_empty() {
            return Err("No valid video segment was created. Check FFmpeg and capture permissions.".into());
        }

        let valid_sys: Vec<PathBuf> = sys_segs
            .into_iter()
            .filter(|p| fs::metadata(p).map(|m| m.len() > 128).unwrap_or(false))
            .collect();
        let valid_mic: Vec<PathBuf> = mic_segs
            .into_iter()
            .filter(|p| fs::metadata(p).map(|m| m.len() > 128).unwrap_or(false))
            .collect();

        let default_vid = app.path().video_dir().unwrap_or_else(|_| std::env::temp_dir());
        let folder = opts
            .save_path
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or(default_vid);
        fs::create_dir_all(&folder).map_err(|e| e.to_string())?;

        let ext = match opts.format.as_str() {
            "mkv" => "mkv",
            "mov" => "mov",
            _ => "mp4",
        };
        let (output, next_index) = choose_output_path(&folder, ext, &opts);
        let exe = resolve_ffmpeg(app);

        let has_sys = !valid_sys.is_empty();
        let has_mic = !valid_mic.is_empty();

        if valid.len() == 1 && ext == "mkv" && !has_sys && !has_mic {
            fs::copy(&valid[0], &output).map_err(|e| e.to_string())?;
        } else {
            let v_list = temp_root.join("concat-v.txt");
            write_concat_file(&v_list, &valid)?;

            let mut args: Vec<String> = vec![
                "-hide_banner".into(),
                "-loglevel".into(), "warning".into(),
                "-f".into(), "concat".into(),
                "-safe".into(), "0".into(),
                "-i".into(), v_list.to_string_lossy().to_string(),
            ];

            let mut audio_inputs = 0usize;
            if has_sys {
                let s_list = temp_root.join("concat-sys.txt");
                write_concat_file(&s_list, &valid_sys)?;
                args.extend([
                    "-f".into(), "concat".into(),
                    "-safe".into(), "0".into(),
                    "-i".into(), s_list.to_string_lossy().to_string(),
                ]);
                audio_inputs += 1;
            }
            if has_mic {
                let m_list = temp_root.join("concat-mic.txt");
                write_concat_file(&m_list, &valid_mic)?;
                args.extend([
                    "-f".into(), "concat".into(),
                    "-safe".into(), "0".into(),
                    "-i".into(), m_list.to_string_lossy().to_string(),
                ]);
                audio_inputs += 1;
            }

            if audio_inputs == 0 {
                args.extend(["-map".into(), "0:v:0".into(), "-c:v".into(), "copy".into()]);
            } else if audio_inputs == 1 {
                args.extend([
                    "-map".into(), "0:v:0".into(),
                    "-map".into(), "1:a:0".into(),
                    "-c:v".into(), "copy".into(),
                    "-c:a".into(), "aac".into(),
                    "-b:a".into(), "192k".into(),
                    "-shortest".into(),
                ]);
            } else {
                args.extend([
                    "-filter_complex".into(),
                    "[1:a][2:a]amix=inputs=2:duration=longest:dropout_transition=2[aout]".into(),
                    "-map".into(), "0:v:0".into(),
                    "-map".into(), "[aout]".into(),
                    "-c:v".into(), "copy".into(),
                    "-c:a".into(), "aac".into(),
                    "-b:a".into(), "192k".into(),
                    "-shortest".into(),
                ]);
            }

            if ext != "mkv" {
                args.extend(["-movflags".into(), "+faststart".into()]);
            }
            args.extend(["-y".into(), output.to_string_lossy().to_string()]);

            let out_res = hidden_cmd(&exe)
                .args(&args)
                .stdin(Stdio::null())
                .output()
                .map_err(|e| format!("Failed to finalize video: {}", e))?;
            if !out_res.status.success() {
                let err_txt = String::from_utf8_lossy(&out_res.stderr);
                return Err(format!("Could not finalize video. {}", err_txt));
            }
        }

        if fs::metadata(&output).map(|m| m.len() < 2048).unwrap_or(true) {
            return Err("The output video is empty or incomplete.".into());
        }
        let _ = fs::remove_dir_all(&temp_root);

        let file_name = output
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        Ok(SaveResult {
            path: output.to_string_lossy().to_string(),
            file_name,
            duration_seconds: (elapsed_ms / 1000).max(1),
            current_index: next_index,
        })
    })();

    if let Ok(mut guard) = state.lock() {
        guard.stopping = false;
    }

    match finalize {
        Ok(res) => {
            let _ = app.emit("recording-saved", &res);
            let _ = app.emit("recording-status-change", json!({ "status": "idle" }));
            Ok(Some(res))
        }
        Err(e) => {
            let _ = app.emit("recording-status-change", json!({ "status": "idle" }));
            Err(format!("{} (raw segments kept in: {})", e, temp_root.display()))
        }
    }
}

fn capture_screen_image_sync() -> Option<String> {
    if !cfg!(target_os = "windows") {
        return None;
    }
    let script = r#"
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$screen = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$bmp = New-Object System.Drawing.Bitmap($screen.Width, $screen.Height)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($screen.Location, [System.Drawing.Point]::Empty, $screen.Size)
$ms = New-Object System.IO.MemoryStream
$bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Jpeg)
$g.Dispose()
$bmp.Dispose()
[Convert]::ToBase64String($ms.ToArray())
"#;
    let output = hidden_cmd("powershell.exe")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", script])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let b64 = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if b64.is_empty() {
        None
    } else {
        Some(format!("data:image/jpeg;base64,{}", b64))
    }
}

#[tauri::command]
fn get_default_save_info(
    app: AppHandle,
    fallback_path: Option<String>,
    prefix: Option<String>,
) -> Value {
    let prefs = read_preferences(&app);
    let stored = prefs.get("savePath").and_then(|v| v.as_str()).map(String::from);
    let default_vid = app
        .path()
        .video_dir()
        .unwrap_or_else(|_| std::env::temp_dir())
        .to_string_lossy()
        .to_string();

    let candidates = [stored.clone(), fallback_path, Some(default_vid.clone())];
    let save_path = candidates
        .into_iter()
        .flatten()
        .find(|p| !p.trim().is_empty() && Path::new(p).exists())
        .unwrap_or(default_vid);

    if stored.is_none() {
        write_preferences(&app, &json!({ "savePath": save_path }));
    }
    let current_index = next_free_index(Path::new(&save_path), prefix.as_deref().unwrap_or(""));
    json!({
        "savePath": save_path,
        "currentIndex": current_index,
        "preferences": read_preferences(&app)
    })
}

#[tauri::command]
fn save_preferences(app: AppHandle, patch: Value) -> Value {
    let mut next = patch.as_object().cloned().unwrap_or_default();
    let create = next
        .remove("createSavePath")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if let Some(val) = next.get("savePath").cloned() {
        let p_str = val.as_str().unwrap_or("").trim().to_string();
        let path = Path::new(&p_str);
        let mut ok = false;
        if !p_str.is_empty() && path.is_absolute() {
            if !path.exists() && create {
                let _ = fs::create_dir_all(path);
            }
            ok = path.exists() && path.is_dir();
        }
        if ok {
            next.insert("savePath".into(), Value::String(p_str));
        } else {
            next.remove("savePath");
        }
    }

    if next.is_empty() {
        read_preferences(&app)
    } else {
        write_preferences(&app, &Value::Object(next))
    }
}

#[tauri::command]
async fn get_open_windows(state: State<'_, AppState>) -> Result<Vec<WindowInfo>, String> {
    let inner = state.inner.clone();
    tauri::async_runtime::spawn_blocking(move || {
        {
            let guard = inner.lock().map_err(|_| "State lock poisoned".to_string())?;
            if guard.active {
                return Ok(guard.windows_cache.1.clone());
            }
            if let Some(at) = guard.windows_cache.0 {
                if at.elapsed() < Duration::from_millis(2500) {
                    return Ok(guard.windows_cache.1.clone());
                }
            }
        }

        if !cfg!(target_os = "windows") {
            return Ok(Vec::new());
        }

        let script = r#"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public class WinList {
  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr extraData);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int count);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
  public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
}
'@
$items = New-Object System.Collections.Generic.List[object]
[WinList]::EnumWindows({ param($h,$l)
  if ([WinList]::IsWindowVisible($h)) {
    $sb = New-Object System.Text.StringBuilder 512
    [void][WinList]::GetWindowText($h,$sb,$sb.Capacity)
    $title = $sb.ToString().Trim()
    $r = New-Object WinList+RECT
    if ($title.Length -gt 0 -and [WinList]::GetWindowRect($h,[ref]$r)) {
      $w = $r.Right - $r.Left; $hgt = $r.Bottom - $r.Top
      if ($w -ge 160 -and $hgt -ge 100) {
        $items.Add([pscustomobject]@{id=$title; title=$title; appName=$title; icon='▣'; width=$w; height=$hgt})
      }
    }
  }
  return $true
}, [IntPtr]::Zero) | Out-Null
$items | Sort-Object title -Unique | ConvertTo-Json -Compress
"#;

        let output = hidden_cmd("powershell.exe")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", script])
            .output();

        let list: Vec<WindowInfo> = match output {
            Ok(out) if out.status.success() => {
                let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
                let parsed: Vec<WindowInfo> = if text.starts_with('[') {
                    serde_json::from_str(&text).unwrap_or_default()
                } else if text.starts_with('{') {
                    serde_json::from_str::<WindowInfo>(&text).map(|w| vec![w]).unwrap_or_default()
                } else {
                    Vec::new()
                };
                parsed
                    .into_iter()
                    .filter(|w| !w.title.is_empty() && w.title != "medialab recording")
                    .collect()
            }
            _ => Vec::new(),
        };

        if let Ok(mut guard) = inner.lock() {
            guard.windows_cache = (Some(Instant::now()), list.clone());
        }
        Ok(list)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn choose_folder(app: AppHandle, prefix: Option<String>) -> Result<Option<Value>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let picked = app
            .dialog()
            .file()
            .set_title("Choose recording folder")
            .blocking_pick_folder();
        let Some(fp) = picked else {
            return Ok(None);
        };
        let path = fp.into_path().map_err(|e| e.to_string())?;
        let folder_str = path.to_string_lossy().to_string();
        write_preferences(&app, &json!({ "savePath": folder_str }));
        let idx = next_free_index(&path, prefix.as_deref().unwrap_or(""));
        Ok(Some(json!({ "savePath": folder_str, "currentIndex": idx })))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn select_region(
    app: AppHandle,
    state: State<'_, AppState>,
    lang: Option<String>,
) -> Result<RegionResult, String> {
    let inner = state.inner.clone();
    let app_clone = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(main_win) = app_clone.get_webview_window("main") {
            let _ = main_win.minimize();
            std::thread::sleep(Duration::from_millis(280));
        }

        let image = capture_screen_image_sync();
        let (lx, ly, lw, lh) = if let Ok(Some(mon)) = app_clone.primary_monitor() {
            let scale = mon.scale_factor();
            (
                mon.position().x as f64 / scale,
                mon.position().y as f64 / scale,
                mon.size().width as f64 / scale,
                mon.size().height as f64 / scale,
            )
        } else {
            (0.0, 0.0, 1920.0, 1080.0)
        };

        let (tx, rx) = mpsc::channel::<RegionResult>();
        {
            let mut guard = inner.lock().map_err(|_| "State lock poisoned".to_string())?;
            guard.region_tx = Some(tx);
            guard.overlay_payload = Some(OverlayStatePayload {
                image,
                origin_x: lx.round() as i32,
                origin_y: ly.round() as i32,
                lang: lang.unwrap_or_else(|| "en".to_string()),
            });
        }

        close_window(&app_clone, "overlay");
        if let Ok(win) = WebviewWindowBuilder::new(&app_clone, "overlay", WebviewUrl::App("overlay.html".into()))
            .title("Region picker")
            .position(lx, ly)
            .inner_size(lw, lh)
            .decorations(false)
            .transparent(true)
            .resizable(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .shadow(false)
            .build()
        {
            let _ = win.set_content_protected(true);
            let _ = win.set_focus();
        }

        let res = rx.recv_timeout(Duration::from_secs(120)).unwrap_or(RegionResult {
            x: None,
            y: None,
            width: None,
            height: None,
            cancelled: Some(true),
        });

        close_window(&app_clone, "overlay");
        if let Ok(mut guard) = inner.lock() {
            guard.region_tx = None;
            guard.overlay_payload = None;
        }
        if let Some(main_win) = app_clone.get_webview_window("main") {
            let _ = main_win.unminimize();
            let _ = main_win.show();
            let _ = main_win.set_focus();
        }
        Ok(res)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn get_overlay_state(state: State<'_, AppState>) -> Option<OverlayStatePayload> {
    state.inner.lock().ok().and_then(|g| g.overlay_payload.clone())
}

#[tauri::command]
fn submit_region_selection(
    app: AppHandle,
    state: State<'_, AppState>,
    result: RegionResult,
) {
    if result.cancelled != Some(true) {
        let _ = app.emit("region-updated", &result);
    }
    if let Ok(mut guard) = state.inner.lock() {
        if let Some(tx) = guard.region_tx.take() {
            let _ = tx.send(result);
        }
    }
}

#[tauri::command]
fn get_hud_state(state: State<'_, AppState>) -> HudStatePayload {
    if let Ok(guard) = state.inner.lock() {
        let elapsed = guard.elapsed_ms
            + if !guard.paused {
                guard.started_at.map(|s| s.elapsed().as_millis() as u64).unwrap_or(0)
            } else {
                0
            };
        let lang = guard
            .options
            .as_ref()
            .and_then(|o| o.lang.clone())
            .unwrap_or_else(|| "en".to_string());
        HudStatePayload {
            paused: guard.paused,
            elapsed_ms: elapsed,
            lang,
        }
    } else {
        HudStatePayload {
            paused: false,
            elapsed_ms: 0,
            lang: "en".to_string(),
        }
    }
}

#[tauri::command]
fn set_hud_compact(app: AppHandle, compact: bool) {
    if let Some(win) = app.get_webview_window("hud") {
        let (w, h) = if compact { (155.0, 44.0) } else { (520.0, 56.0) };
        let _ = win.set_size(Size::Logical(LogicalSize::new(w, h)));
    }
}

#[tauri::command]
async fn start_record(
    app: AppHandle,
    state: State<'_, AppState>,
    mut options: RecordOptions,
) -> Result<Value, String> {
    let inner = state.inner.clone();
    let app_clone = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if options.mode == "region" && options.region.is_none() {
            return Err("Select a recording region first.".into());
        }
        if options.mode == "window" && options.window_title.as_deref().unwrap_or("").trim().is_empty() {
            return Err("Select an open application window first.".into());
        }
        if options.save_path.as_deref().unwrap_or("").trim().is_empty() {
            let prefs = read_preferences(&app_clone);
            let stored = prefs.get("savePath").and_then(|v| v.as_str()).map(String::from);
            let default_vid = app_clone
                .path()
                .video_dir()
                .unwrap_or_else(|_| std::env::temp_dir())
                .to_string_lossy()
                .to_string();
            options.save_path = Some(stored.unwrap_or(default_vid));
        }
        if let Some(ref sp) = options.save_path {
            if Path::new(sp).exists() {
                write_preferences(&app_clone, &json!({ "savePath": sp }));
            }
        }

        {
            let mut guard = inner.lock().map_err(|_| "State lock poisoned".to_string())?;
            if guard.active {
                return Err("A recording is already active.".into());
            }
            guard.options = Some(options);
            guard.segments.clear();
            guard.sys_audio_segments.clear();
            guard.mic_audio_segments.clear();
            guard.segment_index = 0;
            guard.elapsed_ms = 0;
            guard.started_at = Some(Instant::now());
            guard.active = true;
            guard.paused = false;
        }

        if let Err(e) = start_segment_internal(&app_clone, &inner) {
            if let Ok(mut guard) = inner.lock() {
                guard.active = false;
                guard.paused = false;
                if let Some(child) = guard.ffmpeg.take() {
                    kill_child_now(child);
                }
                let sh = guard.sys_audio_handle.take();
                let mh = guard.mic_audio_handle.take();
                drop(guard);
                let _ = stop_audio_handle(sh);
                let _ = stop_audio_handle(mh);
            }
            close_window(&app_clone, "hud");
            return Err(e);
        }

        show_hud_window(&app_clone);
        if let Some(main_win) = app_clone.get_webview_window("main") {
            let _ = main_win.minimize();
        }
        let _ = app_clone.emit("recording-status-change", json!({ "status": "recording" }));
        Ok(json!({ "ok": true }))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn pause_record(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let inner = state.inner.clone();
    tauri::async_runtime::spawn_blocking(move || pause_internal(&app, &inner))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn resume_record(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let inner = state.inner.clone();
    tauri::async_runtime::spawn_blocking(move || resume_internal(&app, &inner))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn stop_save(app: AppHandle, state: State<'_, AppState>) -> Result<Option<SaveResult>, String> {
    let inner = state.inner.clone();
    tauri::async_runtime::spawn_blocking(move || stop_save_internal(&app, &inner))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn cancel_record(app: AppHandle, state: State<'_, AppState>) -> Result<Value, String> {
    let inner = state.inner.clone();
    tauri::async_runtime::spawn_blocking(move || {
        cancel_internal(&app, &inner)?;
        Ok(json!({ "cancelled": true }))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn hud_toggle_pause(app: AppHandle, state: State<'_, AppState>) {
    let inner = state.inner.clone();
    std::thread::spawn(move || {
        let is_paused = inner.lock().map(|g| g.paused).unwrap_or(false);
        let res = if is_paused {
            resume_internal(&app, &inner)
        } else {
            pause_internal(&app, &inner)
        };
        if let Err(e) = res {
            let _ = app.emit("recording-error", e);
        }
    });
}

#[tauri::command]
fn hud_stop_save(app: AppHandle, state: State<'_, AppState>) {
    let inner = state.inner.clone();
    std::thread::spawn(move || {
        if let Err(e) = stop_save_internal(&app, &inner) {
            let _ = app.emit("recording-error", e);
        }
    });
}

#[tauri::command]
fn hud_cancel(app: AppHandle, state: State<'_, AppState>) {
    let inner = state.inner.clone();
    std::thread::spawn(move || {
        if let Err(e) = cancel_internal(&app, &inner) {
            let _ = app.emit("recording-error", e);
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_state = AppState::new();
    let temp_root_for_cleanup = app_state.inner.lock().map(|g| g.temp_root.clone()).unwrap_or_else(|_| std::env::temp_dir());
    clean_stale_temp_folders(&temp_root_for_cleanup);

    let r_shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyR);
    let p_shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyP);

    tauri::Builder::default()
        .manage(app_state)
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    let state = app.state::<AppState>();
                    let inner = state.inner.clone();
                    let app_clone = app.clone();
                    if shortcut == &r_shortcut {
                        std::thread::spawn(move || {
                            let is_active = inner.lock().map(|g| g.active).unwrap_or(false);
                            if is_active {
                                if let Err(e) = stop_save_internal(&app_clone, &inner) {
                                    let _ = app_clone.emit("recording-error", e);
                                }
                            } else {
                                let _ = app_clone.emit("hotkey-start", json!({}));
                            }
                        });
                    } else if shortcut == &p_shortcut {
                        std::thread::spawn(move || {
                            let (is_active, is_paused) = inner
                                .lock()
                                .map(|g| (g.active, g.paused))
                                .unwrap_or((false, false));
                            if !is_active {
                                return;
                            }
                            let res = if is_paused {
                                resume_internal(&app_clone, &inner)
                            } else {
                                pause_internal(&app_clone, &inner)
                            };
                            if let Err(e) = res {
                                let _ = app_clone.emit("recording-error", e);
                            }
                        });
                    }
                })
                .build(),
        )
        .setup(move |app| {
            let _ = app.global_shortcut().register(r_shortcut);
            let _ = app.global_shortcut().register(p_shortcut);

            let app_handle = app.handle().clone();
            let inner = app.state::<AppState>().inner.clone();
            std::thread::spawn(move || {
                let exe = resolve_ffmpeg(&app_handle);
                let _ = probe_hardware_encoder_sync(&exe, &app_handle, &inner);
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let WindowEvent::CloseRequested { .. } = event {
                    let app = window.app_handle().clone();
                    let state = app.state::<AppState>();
                    let is_active = state.inner.lock().map(|g| g.active).unwrap_or(false);
                    if is_active {
                        let inner = state.inner.clone();
                        let _ = stop_save_internal(&app, &inner);
                    }
                    close_window(&app, "hud");
                    close_window(&app, "overlay");
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_default_save_info,
            save_preferences,
            get_open_windows,
            choose_folder,
            select_region,
            get_overlay_state,
            submit_region_selection,
            get_hud_state,
            set_hud_compact,
            start_record,
            pause_record,
            resume_record,
            stop_save,
            cancel_record,
            hud_toggle_pause,
            hud_stop_save,
            hud_cancel
        ])
        .run(tauri::generate_context!())
        .expect("error while running medialab recording");
}
