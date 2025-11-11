#![allow(
    clippy::needless_continue,
    clippy::redundant_else,
    clippy::similar_names,
    clippy::significant_drop_tightening,
    clippy::needless_pass_by_value,
    clippy::items_after_statements,
    clippy::ignored_unit_patterns,
    clippy::branches_sharing_code
)]
use std::time::{Duration, Instant};

use crate::config::config as global_config;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{lookup_host, TcpStream, UdpSocket};
use tokio::sync::Mutex as TokioMutex;
use tokio::time::timeout as tokio_timeout;

use crate::error::MelsecError;
use crate::mc_frame::detect_frame;

// Type alias to reduce complex type warnings from Clippy
type TargetConns = TokioMutex<
    std::collections::HashMap<String, std::sync::Arc<TokioMutex<Option<(TcpStream, Instant)>>>>,
>;

// Per-target connection pool used by TCP send/recv
static TARGET_CONNS: Lazy<TargetConns> = Lazy::new(|| TokioMutex::new(HashMap::new()));

const MAX_FRAME_LEN: usize = 65535;

fn hex_dump(b: &[u8]) -> String {
    b.iter()
        .map(|x| format!("{x:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

// helper that performs write+read loop on an established stream
async fn perform_stream_io(
    mut stream: TcpStream,
    payload: &[u8],
    timeout: Option<Duration>,
    expected_serial: u16,
) -> Result<(Vec<u8>, TcpStream), String> {
    // write payload (apply optional timeout)
    if let Some(dur) = timeout {
        match tokio_timeout(dur, stream.write_all(payload)).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(e.to_string()),
            Err(e) => return Err(e.to_string()),
        }
    } else if let Err(e) = stream.write_all(payload).await {
        return Err(e.to_string());
    }
    // Log outgoing TCP payload as hex for debugging
    log::debug!("[MC TCP send] {}", hex_dump(payload));
    // also emit to stderr to ensure visibility during ad-hoc runs
    eprintln!("[MC TCP send] {}", hex_dump(payload));

    let mut buf: Vec<u8> = Vec::new();
    let mut tmp = [0u8; 4096];

    loop {
        // read with optional timeout
        let nres: Result<usize, std::io::Error> = if let Some(dur) = timeout {
            match tokio_timeout(dur, stream.read(&mut tmp)).await {
                Ok(Ok(n)) => Ok(n),
                Ok(Err(e)) => Err(e),
                Err(_) => Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout")),
            }
        } else {
            stream.read(&mut tmp).await
        };

        match nres {
            Ok(0) => break, // remote closed
            Ok(n) => {
                buf.extend_from_slice(&tmp[..n]);
                // try parse complete frames
                loop {
                    // Use centralized frame detection
                    match detect_frame(&buf) {
                        Ok(Some((frame_len, _header_len, serial_opt))) => {
                            if frame_len == 0 || frame_len > MAX_FRAME_LEN {
                                let _ = buf.drain(..1).count();
                                continue;
                            }
                            if buf.len() < frame_len {
                                break;
                            }
                            let serial = serial_opt.unwrap_or(0);
                            if expected_serial == 0 || serial == expected_serial {
                                let frame = buf.drain(..frame_len).collect::<Vec<u8>>();
                                // Log incoming TCP frame as hex for debugging
                                log::debug!("[MC TCP recv] {}", hex_dump(&frame));
                                // also emit to stderr to ensure visibility during ad-hoc runs
                                eprintln!("[MC TCP recv] {}", hex_dump(&frame));
                                return Ok((frame, stream));
                            } else {
                                let _dropped = buf.drain(..frame_len).count();
                                continue;
                            }
                        }
                        Ok(None) => break, // need more bytes or unrecognized
                        Err(_e) => {
                            // malformed header: skip one byte and continue scanning
                            let _ = buf.drain(..1).count();
                            continue;
                        }
                    }
                }
            }
            Err(e) => {
                return Err(e.to_string());
            }
        }
    }
    Err("remote closed without producing matching frame".to_string())
}

/// Async: Send `payload` to `addr` (TCP) and read the response.
/// `addr` is a string like "host:port". `timeout` is an optional duration for read/write.
///
/// Returns the raw response bytes (as received from the socket).
///
/// # Errors
///
/// Returns `Err(MelsecError)` when address resolution, connect, write, or read fails.
pub async fn send_and_recv_tcp(
    addr: &str,
    payload: &[u8],
    timeout: Option<Duration>,
) -> Result<Vec<u8>, MelsecError> {
    // perform_stream_io is a top-level helper defined earlier

    let mut last_err: Option<String> = None;

    // resolve addresses asynchronously
    let addrs = lookup_host(addr)
        .await
        .map_err(|e| MelsecError::Protocol(format!("bad address {addr}: {e}")))?;

    // determine expected serial number from outgoing payload (bytes 2..3 little-endian)
    let expected_serial = if payload.len() >= 4 {
        u16::from_le_bytes([payload[2], payload[3]])
    } else {
        0
    };

    // Get or create the per-target connection mutex for this addr string
    let conn_mutex: Arc<TokioMutex<Option<(TcpStream, Instant)>>> = {
        let mut map = TARGET_CONNS.lock().await;
        map.entry(addr.to_string())
            .or_insert_with(|| Arc::new(TokioMutex::new(None)))
            .clone()
    };

    // Lock the per-target connection. This lock serializes traffic to the same addr and
    // provides access to a reusable TcpStream stored inside the Option.
    let mut conn_guard = conn_mutex.lock().await; // guard: &mut Option<TcpStream>

    // If we already have a cached connection for this addr, try reuse first.
    // Respect an idle timeout to avoid using very-old connections.
    let idle_duration = Duration::from_secs(global_config().melsec_conn_idle_secs);
    if let Some((cached_stream, last_used)) = conn_guard.take() {
        if last_used.elapsed() <= idle_duration {
            match perform_stream_io(cached_stream, payload, timeout, expected_serial).await {
                Ok((frame, stream)) => {
                    // put back the usable stream for future reuse
                    *conn_guard = Some((stream, Instant::now()));
                    return Ok(frame);
                }
                Err(e) => {
                    // on failure invalidate and proceed to attempt fresh connections
                    *conn_guard = None;
                    last_err = Some(e);
                }
            }
        } else {
            // cached connection expired due to idle timeout; drop it and continue to connect
            *conn_guard = None;
        }
    }

    for remote in addrs {
        // try to connect with a short connect timeout
        let connect_res = tokio_timeout(Duration::from_secs(3), TcpStream::connect(remote)).await;
        let stream = match connect_res {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => {
                return Err(MelsecError::Protocol(format!("connect error: {e}")));
            }
            Err(e) => {
                return Err(MelsecError::Protocol(format!("connect timeout/error: {e}")));
            }
        };

        // perform write+read on the established stream
        match perform_stream_io(stream, payload, timeout, expected_serial).await {
            Ok((frame, stream)) => {
                // cache the successful stream for future reuse
                *conn_guard = Some((stream, Instant::now()));
                return Ok(frame);
            }
            Err(e) => {
                // invalidate stored connection on failure
                *conn_guard = None;
                last_err = Some(e);
                continue;
            }
        }
    }

    Err(MelsecError::Protocol(format!(
        "failed to send TCP to {addr}: {last_err:?}"
    )))
}

// Async UDP send/recv
/// Async: Send `payload` (UDP) and receive a single response packet. Works with IPv4/IPv6.
///
/// # Errors
///
/// Returns `Err(MelsecError)` when address resolution, bind, send, or receive fails.
pub async fn send_and_recv_udp(
    addr: &str,
    payload: &[u8],
    timeout: Option<Duration>,
) -> Result<Vec<u8>, MelsecError> {
    let addrs = lookup_host(addr)
        .await
        .map_err(|e| MelsecError::Protocol(format!("bad address {addr}: {e}")))?;
    // determine expected serial from outgoing payload (bytes 2..3 little-endian)
    let expected_serial = if payload.len() >= 4 {
        u16::from_le_bytes([payload[2], payload[3]])
    } else {
        0
    };
    let max_attempts: usize = global_config().melsec_udp_recv_attempts;

    for remote in addrs {
        let bind_addr = if remote.is_ipv4() {
            "0.0.0.0:0"
        } else {
            "[::]:0"
        };
        match UdpSocket::bind(bind_addr).await {
            Ok(sock) => {
                // attempt send/receive up to max_attempts
                for attempt in 0..max_attempts {
                    if let Err(e) = sock.send_to(payload, remote).await {
                        let dump_env = global_config().melsec_dump_on_error;
                        if dump_env {
                            let payload_hex = hex_dump(payload);
                            log::error!("[MC UDP ERROR] send_to failed addr={} payload={} attempt={} err={}", addr, payload_hex, attempt+1, e);
                        }
                        continue;
                    }

                    let mut buf = vec![0u8; 65535];
                    loop {
                        let recv_res: Result<(usize, std::net::SocketAddr), std::io::Error> =
                            if let Some(dur) = timeout {
                                match tokio_timeout(dur, sock.recv_from(&mut buf)).await {
                                    Ok(Ok(r)) => Ok(r),
                                    Ok(Err(e)) => Err(e),
                                    Err(_) => Err(std::io::Error::new(
                                        std::io::ErrorKind::TimedOut,
                                        "timeout",
                                    )),
                                }
                            } else {
                                sock.recv_from(&mut buf).await
                            };

                        match recv_res {
                            Ok((n, _src)) => {
                                buf.truncate(n);
                                let serial = if buf.len() >= 4 {
                                    u16::from_le_bytes([buf[2], buf[3]])
                                } else {
                                    0
                                };
                                if expected_serial == 0 || serial == expected_serial {
                                    // Optionally log received payloads (hex) when requested
                                    if global_config().log_mc_payloads {
                                        log::debug!("[MC PAYLOAD recv] {}", hex_dump(&buf));
                                    }
                                    return Ok(buf);
                                } else {
                                    let dump_env = global_config().melsec_dump_on_error;
                                    if dump_env {
                                        let payload_hex = hex_dump(payload);
                                        log::warn!("[MC UDP ERROR] serial mismatch (expected=0x{:04X} got=0x{:04X}), discarding packet addr={} payload={} pkt_serial=0x{:04X}", expected_serial, serial, addr, payload_hex, serial);
                                    }
                                    continue;
                                }
                            }
                            Err(e) => {
                                let dump_env = global_config().melsec_dump_on_error;
                                if dump_env {
                                    let payload_hex = hex_dump(payload);
                                    log::error!("[MC UDP ERROR] addr={} payload={} recv error on attempt {}: {}", addr, payload_hex, attempt+1, e);
                                }
                                break;
                            }
                        }
                    }
                }
            }
            Err(e) => {
                let dump_env = global_config().melsec_dump_on_error;
                if dump_env {
                    log::error!("[MC UDP ERROR] bind failed {} err={}", bind_addr, e);
                }
                continue;
            }
        }
    }

    Err(MelsecError::Protocol(format!(
        "failed to send UDP to {addr}"
    )))
}
