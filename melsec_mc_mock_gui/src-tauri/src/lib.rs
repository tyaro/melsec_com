// Tauri backend with embedded tokio runtime and MockServer integration.
use std::sync::{Arc, Mutex};

use anyhow::Result;
use melsec_mc_mock::MockServer;
use serde::Serialize;
use std::io::Write;
use std::path::PathBuf;
use tauri::Emitter;
use tokio::sync::RwLock;
use tracing::{debug, info, warn, error};

#[derive(Clone, Serialize)]
struct MonitorPayload {
    key: String,
    addr: usize,
    vals: Vec<u16>,
}

struct AppState {
    rt: tokio::runtime::Runtime,
    server: Arc<RwLock<MockServer>>,
    monitor_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    monitor_cfg: Arc<Mutex<Option<(String, usize, usize, u64)>>>,
    // track mock listener handles so we don't start multiple listeners accidentally
    mock_handles: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>>,
}

impl AppState {
    fn new() -> Self {
        let rt = tokio::runtime::Runtime::new().expect("create tokio runtime");
        let server = MockServer::new();
        Self {
            rt,
            server: Arc::new(RwLock::new(server)),
            monitor_handle: Arc::new(Mutex::new(None)),
            monitor_cfg: Arc::new(Mutex::new(None)),
            mock_handles: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

// Start internal mock server: bind TCP and optional UDP
#[tauri::command]
async fn start_mock(window: tauri::Window, state: tauri::State<'_, Arc<AppState>>, ip: String, tcp_port: u16, udp_port: Option<u16>, tim_await_ms: Option<u64>) -> Result<(), String> {
    let app = state.inner();
    if let Some(ms) = tim_await_ms {
        std::env::set_var("MELSEC_MOCK_TIM_AWAIT_MS", ms.to_string());
    }
    // if mock listeners already running, do nothing (idempotent)
    {
        let mut handles = app.mock_handles.lock().unwrap();
        // clean up finished handles
        handles.retain(|h| !h.is_finished());
        if !handles.is_empty() {
            // already running
            let _ = window.emit("server-status", "起動中");
            return Ok(());
        }
    }

    let server = app.server.clone();
    let bind_addr = format!("{}:{}", ip, tcp_port);

    // start tcp listener task
    let srv_for_tcp = server.clone();
    let tcp_bind = bind_addr.clone();
    let tcp_handle = tokio::spawn(async move {
        if let Ok(listener) = tokio::net::TcpListener::bind(&tcp_bind).await {
            let srv_run = srv_for_tcp.read().await.clone();
            let _ = tokio::spawn(async move { let _ = srv_run.run_listener_on(listener).await; }).await;
            } else {
                warn!("[TAURI BACKEND] failed to bind tcp mock at {}", tcp_bind);
            }
    });

    // start udp listener if requested
    let udp_handle_opt = if let Some(port) = udp_port {
        let udp_bind = format!("0.0.0.0:{}", port);
        let srv_for_udp = server.clone();
        Some(tokio::spawn(async move {
            if let Ok(_) = tokio::net::UdpSocket::bind(&udp_bind).await {
                let srv2 = srv_for_udp.read().await.clone();
                let _ = tokio::spawn(async move { let _ = srv2.run_udp_listener(&udp_bind).await; }).await;
                } else {
                    warn!("[TAURI BACKEND] failed to bind udp mock at {}", udp_bind);
            }
        }))
    } else { None };

    // store handles so we can prevent duplicates and stop later
    {
        let mut handles = app.mock_handles.lock().unwrap();
        handles.push(tcp_handle);
        if let Some(h) = udp_handle_opt { handles.push(h); }
    }

    let _ = window.emit("server-status", "起動中");
    Ok(())
}

#[tauri::command]
async fn stop_mock(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    let app = state.inner();
    let mut handles = app.mock_handles.lock().unwrap();
    for h in handles.drain(..) {
        h.abort();
    }
    Ok(())
}

#[tauri::command]
async fn set_words(window: tauri::Window, state: tauri::State<'_, Arc<AppState>>, key: String, addr: usize, words: Vec<u16>) -> Result<(), String> {
    let app = state.inner();
    let server = app.server.clone();
    let monitor_cfg = app.monitor_cfg.clone();

    // perform the write on the shared MockServer instance
    let mut s = server.write().await;
    debug!("[TAURI BACKEND] set_words called key={} addr={} words={:?}", key, addr, words);
    // persist debug trace to file to ensure visibility even if stderr is not shown
    {
        let mut debug_path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        debug_path.push("tauri_debug.log");
    debug!("[TAURI BACKEND] writing debug to {:?}", debug_path);
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&debug_path) {
            let ts = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) { Ok(d) => d.as_millis(), Err(_) => 0 };
            let _ = writeln!(f, "{} [SET_WORDS] key={} addr={} words={:?}", ts, key, addr, words);
        }
    }
    s.set_words(&key, addr, &words).await;
    // read back the same range to verify the write took effect
    let readback = s.get_words(&key, addr, words.len()).await;
    debug!("[TAURI BACKEND] set_words readback key={} addr={} len={} => {:?}", key, addr, words.len(), readback);
    {
        let mut debug_path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        debug_path.push("tauri_debug.log");
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&debug_path) {
            let ts = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) { Ok(d) => d.as_millis(), Err(_) => 0 };
            let _ = writeln!(f, "{} [SET_WORDS_READBACK] key={} addr={} len={} readback={:?}", ts, key, addr, words.len(), readback);
        }
    }
    // push immediate monitor if configured
    if let Some((mkey, maddr, mcount, _interval)) = monitor_cfg.lock().unwrap().clone() {
        let v = s.get_words(&mkey, maddr, mcount).await;
    debug!("[TAURI BACKEND] set_words trigger monitor emit key={} addr={} vals={:?}", mkey, maddr, v);
        {
            let mut debug_path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            debug_path.push("tauri_debug.log");
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&debug_path) {
                let ts = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) { Ok(d) => d.as_millis(), Err(_) => 0 };
                let _ = writeln!(f, "{} [SET_WORDS_EMIT] key={} addr={} vals={:?}", ts, mkey, maddr, v);
            }
        }
        let payload = MonitorPayload { key: mkey.clone(), addr: maddr, vals: v };
        let emit_res = window.emit("monitor", payload);
        if let Err(e) = emit_res {
            error!("[TAURI BACKEND] emit monitor failed: {:?}", e);
        }
    }
    Ok(())
}

#[tauri::command]
async fn get_words(state: tauri::State<'_, Arc<AppState>>, key: String, addr: usize, count: usize) -> Result<Vec<u16>, String> {
    let app = state.inner();
    debug!("[TAURI BACKEND] get_words called key={} addr={} count={}", key, addr, count);
    {
        let mut debug_path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        debug_path.push("tauri_debug.log");
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&debug_path) {
            let ts = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) { Ok(d) => d.as_millis(), Err(_) => 0 };
            let _ = writeln!(f, "{} [GET_WORDS] key={} addr={} count={}", ts, key, addr, count);
        }
    }
    let server = app.server.clone();
    let s = server.read().await;
    let v = s.get_words(&key, addr, count).await;
    {
        let mut debug_path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        debug_path.push("tauri_debug.log");
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&debug_path) {
            let ts = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) { Ok(d) => d.as_millis(), Err(_) => 0 };
            let _ = writeln!(f, "{} [GET_WORDS_RET] key={} addr={} vals={:?}", ts, key, addr, v);
        }
    }
    Ok(v)
}

#[tauri::command]
fn start_monitor(window: tauri::Window, state: tauri::State<'_, Arc<AppState>>, key: String, addr: usize, count: usize, interval_ms: u64) -> Result<(), String> {
    let app = state.inner();
    let server = app.server.clone();
    // store cfg
    *app.monitor_cfg.lock().unwrap() = Some((key.clone(), addr, count, interval_ms));
    let win = window.clone();
    let h = app.rt.spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(interval_ms));
        loop {
            interval.tick().await;
            let s = server.read().await;
            let v = s.get_words(&key, addr, count).await;
            if let Ok(dur) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
                debug!("[TAURI BACKEND] Monitor send ts={} key={} addr={} vals={:?}", dur.as_millis(), key, addr, v);
            }
            let payload = MonitorPayload { key: key.clone(), addr, vals: v };
            let _ = win.emit("monitor", payload.clone());
        }
    });
    *app.monitor_handle.lock().unwrap() = Some(h);
    Ok(())
}

#[tauri::command]
fn stop_monitor(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    let app = state.inner();
    if let Some(h) = app.monitor_handle.lock().unwrap().take() {
        h.abort();
    }
    *app.monitor_cfg.lock().unwrap() = None;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = Arc::new(AppState::new());
    // print current working directory so we can locate relative debug files
    match std::env::current_dir() {
        Ok(p) => info!("[TAURI BACKEND] cwd={:?}", p),
        Err(e) => info!("[TAURI BACKEND] cwd error: {:?}", e),
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            start_mock,
            stop_mock,
            set_words,
            get_words,
            start_monitor,
            stop_monitor,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
