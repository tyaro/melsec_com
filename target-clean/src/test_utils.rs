use std::time::{SystemTime, UNIX_EPOCH};
use tracing::info;

/// Print a concise test banner so it's easy to see what is running in CI logs.
pub fn announce(name: &str, description: &str) {
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).ok();
    if let Some(d) = ts {
        info!("[TEST START] {name} - {description} (ts={})", d.as_secs());
    } else {
        info!("[TEST START] {name} - {description}");
    }
}
