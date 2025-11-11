// Tauri backend with embedded tokio runtime and MockServer integration.
use std::sync::{Arc, Mutex};

use anyhow::Result;
use melsec_mc::device::parse_device_and_address;
use melsec_mc_mock::MockServer;
use serde::Serialize;
use std::io::Write;
use tauri::Emitter;
use tokio::sync::{Mutex as AsyncMutex, RwLock};
use tracing::{debug, info};

#[derive(Clone, Serialize)]
struct MonitorPayload {
    key: String,
    addr: usize,
    vals: Vec<u16>,
}

struct AppState {
    rt: tokio::runtime::Runtime,
    server: Arc<RwLock<MockServer>>,
    monitor_handle: Arc<AsyncMutex<Option<tokio::task::JoinHandle<()>>>>,
    // handles for spawned TCP/UDP listener tasks so they can be aborted by stop_mock
    listener_handles: Arc<AsyncMutex<Vec<tokio::task::JoinHandle<()>>>>,
    // monitor_cfg: (device_key_symbol, addr, interval_ms) - count is fixed to 30
    monitor_cfg: Arc<Mutex<Option<(String, usize, u64)>>>,
}

impl AppState {
    fn new() -> Self {
        let rt = tokio::runtime::Runtime::new().expect("create tokio runtime");
        let server = MockServer::new();
        Self {
            rt,
            server: Arc::new(RwLock::new(server)),
            monitor_handle: Arc::new(AsyncMutex::new(None)),
            listener_handles: Arc::new(AsyncMutex::new(Vec::new())),
            monitor_cfg: Arc::new(Mutex::new(None)),
        }
    }
}

// Start internal mock server: bind TCP and optional UDP
#[tauri::command]
fn start_mock(
    state: tauri::State<'_, Arc<AppState>>,
    ip: String,
    tcp_port: u16,
    udp_port: Option<u16>,
    tim_await_ms: Option<u64>,
) -> Result<(), String> {
    let app = state.inner();
    if let Some(ms) = tim_await_ms {
        std::env::set_var("MELSEC_MOCK_TIM_AWAIT_MS", ms.to_string());
    }
    let server = app.server.clone();
    let handles = app.listener_handles.clone();
    let bind_addr = format!("{}:{}", ip, tcp_port);
    app.rt.spawn(async move {
        let srv_clone = server.read().await.clone();
        if let Ok(listener) = tokio::net::TcpListener::bind(&bind_addr).await {
            let srv_run = srv_clone.clone();
            // spawn tcp listener and record handle
            let h = tokio::spawn(async move {
                let _ = srv_run.run_listener_on(listener).await;
            });
            // async lock to push handle
            handles.lock().await.push(h);
        }
        if let Some(port) = udp_port {
            let udp_bind = format!("0.0.0.0:{}", port);
            if let Ok(_sock) = tokio::net::UdpSocket::bind(&udp_bind).await {
                let srv2 = server.read().await.clone();
                let b = udp_bind.clone();
                // spawn udp listener and record handle
                let h2 = tokio::spawn(async move {
                    let _ = srv2.run_udp_listener(&b).await;
                });
                handles.lock().await.push(h2);
            }
        }
    });
    Ok(())
}

#[tauri::command]
async fn stop_mock(
    window: tauri::Window,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let app = state.inner();
    // take handles and abort them
    let mut hs = app.listener_handles.lock().await;
    for h in hs.drain(..) {
        h.abort();
    }
    // clear the mock server internal DeviceMap to reset memory
    {
        let srv = app.server.clone();
        let s = srv.write().await;
        let store = s.store.clone();
        // persist snapshot before clearing
        let _ = s.save_snapshot("./sled_db/device_map_snapshot.json").await;
        let mut dm = store.write().await;
        dm.clear();
    }
    // notify frontend about stopped status
    let _ = window.emit("server-status", "停止中");
    Ok(())
}

#[tauri::command]
fn set_words(
    window: tauri::Window,
    state: tauri::State<'_, Arc<AppState>>,
    key: String,
    addr: usize,
    words: Vec<u16>,
) -> Result<(), String> {
    let app = state.inner();
    let server = app.server.clone();
    let monitor_cfg = app.monitor_cfg.clone();
    // log invocation and persist debug trace to cwd/tauri_debug.log
    debug!(
        "[TAURI BACKEND] set_words called key={} addr={} words={:?}",
        key, addr, words
    );
    {
        let mut debug_path = std::env::temp_dir();
        debug_path.push("melsec_tauri_debug.log");
        debug!("[TAURI BACKEND] writing debug to {:?}", debug_path);
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&debug_path)
        {
            let ts = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
                Ok(d) => d.as_millis(),
                Err(_) => 0,
            };
            let _ = writeln!(
                f,
                "{} [SET_WORDS] key={} addr={} words={:?}",
                ts, key, addr, words
            );
        }
    }

    app.rt.block_on(async move {
        let s = server.write().await;
        s.set_words(&key, addr, &words).await;
        // read back and log
        let readback = s.get_words(&key, addr, words.len()).await;
        debug!(
            "[TAURI BACKEND] set_words readback key={} addr={} len={} => {:?}",
            key,
            addr,
            words.len(),
            readback
        );
        {
            let mut debug_path = std::env::temp_dir();
            debug_path.push("melsec_tauri_debug.log");
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&debug_path)
            {
                let ts = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
                    Ok(d) => d.as_millis(),
                    Err(_) => 0,
                };
                let _ = writeln!(
                    f,
                    "{} [SET_WORDS_READBACK] key={} addr={} len={} readback={:?}",
                    ts,
                    key,
                    addr,
                    words.len(),
                    readback
                );
            }
        }
        // push immediate monitor if configured
        let monitor_snapshot = { monitor_cfg.lock().unwrap().clone() };
        if let Some((mkey, maddr, _interval)) = monitor_snapshot {
            // fixed monitor count of 30
            let mcount = 30usize;
            let v = s.get_words(&mkey, maddr, mcount).await;
            debug!(
                "[TAURI BACKEND] set_words trigger monitor emit key={} addr={} vals={:?}",
                mkey, maddr, v
            );
            {
                let mut debug_path = std::env::temp_dir();
                debug_path.push("melsec_tauri_debug.log");
                if let Ok(mut f) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&debug_path)
                {
                    let ts =
                        match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
                            Ok(d) => d.as_millis(),
                            Err(_) => 0,
                        };
                    let _ = writeln!(
                        f,
                        "{} [SET_WORDS_EMIT] key={} addr={} vals={:?}",
                        ts, mkey, maddr, v
                    );
                }
            }
            let payload = MonitorPayload {
                key: mkey.clone(),
                addr: maddr,
                vals: v,
            };
            let _ = window.emit("monitor", payload);
        }
        Ok(())
    })
}

#[tauri::command]
fn get_words(
    state: tauri::State<'_, Arc<AppState>>,
    key: String,
    addr: usize,
    count: usize,
) -> Result<Vec<u16>, String> {
    let app = state.inner();
    debug!(
        "[TAURI BACKEND] get_words called key={} addr={} count={}",
        key, addr, count
    );
    {
        let mut debug_path = std::env::temp_dir();
        debug_path.push("melsec_tauri_debug.log");
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&debug_path)
        {
            let ts = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
                Ok(d) => d.as_millis(),
                Err(_) => 0,
            };
            let _ = writeln!(
                f,
                "{} [GET_WORDS] key={} addr={} count={}",
                ts, key, addr, count
            );
        }
    }
    let server = app.server.clone();
    // clone key so we can use original `key` later for logging without moving it into the async block
    let key_for_async = key.clone();
    let v = app.rt.block_on(async move {
        let s = server.read().await;
        s.get_words(&key_for_async, addr, count).await
    });
    {
        let mut debug_path = std::env::temp_dir();
        debug_path.push("melsec_tauri_debug.log");
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&debug_path)
        {
            let ts = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
                Ok(d) => d.as_millis(),
                Err(_) => 0,
            };
            let _ = writeln!(
                f,
                "{} [GET_WORDS_RET] key={} addr={} vals={:?}",
                ts, key, addr, v
            );
        }
    }
    Ok(v)
}

#[tauri::command]
async fn start_monitor(
    window: tauri::Window,
    state: tauri::State<'_, Arc<AppState>>,
    target: String,
    interval_ms: u64,
) -> Result<(), String> {
    // target is combined like "D100" or "W1FFF"; parsing uses device base
    let app = state.inner();
    let server = app.server.clone();
    // parse target using crate device parser
    let (device, addr_u32) =
        parse_device_and_address(&target).map_err(|e| format!("parse target error: {}", e))?;
    let addr = addr_u32 as usize;
    // fixed count = 30
    let count = 30usize;
    let win = window.clone();
    // notify frontend that monitor started
    let _ = win.emit("server-status", "監視中");
    let key = device.symbol_str().to_string();
    // store cfg (store the device symbol key, not the raw target string)
    *app.monitor_cfg.lock().unwrap() = Some((key.clone(), addr, interval_ms));
    let h = app.rt.spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(interval_ms));
        // Do an immediate first poll so frontend shows initial state without waiting
        {
            let s = server.read().await;
            // use symbol `key` with explicit addr so DeviceMap resolves correctly
            let v = s.get_words(&key, addr, count).await;
            let payload = MonitorPayload {
                key: key.clone(),
                addr,
                vals: v,
            };
            let _ = win.emit("monitor", payload.clone());
        }
        loop {
            interval.tick().await;
            let s = server.read().await;
            // use symbol `key` with explicit addr so DeviceMap resolves correctly
            let v = s.get_words(&key, addr, count).await;
            // emit monitor payload to frontend (no console logging)
            let payload = MonitorPayload {
                key: key.clone(),
                addr,
                vals: v,
            };
            let _ = win.emit("monitor", payload.clone());
        }
    });
    *app.monitor_handle.lock().await = Some(h);
    Ok(())
}

#[tauri::command]
async fn stop_monitor(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    let app = state.inner();
    let mut guard = app.monitor_handle.lock().await;
    if let Some(h) = guard.take() {
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
