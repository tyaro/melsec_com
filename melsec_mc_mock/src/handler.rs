use std::sync::Arc;
use tokio::sync::RwLock;

use anyhow::Result;

use crate::device_map::DeviceMap;

/// This file contains the request handling and spec-driven response builder
/// implementations migrated from the previous monolithic `lib.rs`.
// test helpers and unit tests are placed in the bottom `tests` module to avoid
// duplicate module definitions when this file is compiled with the test harness.
pub async fn handle_request_and_apply_store(
    store: &Arc<RwLock<DeviceMap>>,
    req: &melsec_mc::request::McRequest,
) -> Result<Vec<u8>> {
    // data slice contains the request body (command/subcommand/...)
    let data = &req.request_data;
    if data.len() < 4 {
        anyhow::bail!("request too short");
    }
    let command = u16::from_le_bytes([data[0], data[1]]);
    let sub = u16::from_le_bytes([data[2], data[3]]);

    // Try to find a CommandSpec for this numeric command+subcommand. If found
    // prefer the typed CommandSpec and build a response using its response_description.
    let registry_opt = melsec_mc::command_registry::CommandRegistry::global();

    // helpers to read common fields
    // Support both legacy MC3E-style layout (start_addr:3le, device_code:1, count:2)
    // and PLCSeries::R / MC4E layout where start_addr is 4 bytes and device_code is 2 bytes
    let read_start_and_device_and_count = || -> Option<(usize, u64, usize, usize)> {
        // Try to parse both MC4E and MC3E header layouts and pick the one
        // that best matches the available payload bytes (so writes with data
        // choose the layout whose expected payload length fits). For read-only
        // requests (no payload) prefer MC4E when full header present, else MC3E.

        // Parse MC4E candidate if buffer is long enough for header
        let mc4 = if data.len() >= 12 {
            let s0 = data[4] as u32;
            let s1 = data[5] as u32;
            let s2 = data[6] as u32;
            let s3 = data[7] as u32;
            let start4 = ((s3 << 24) | (s2 << 16) | (s1 << 8) | s0) as usize;
            let device_code4 = u64::from(u16::from_le_bytes([data[8], data[9]]));
            let count4 = u16::from_le_bytes([data[10], data[11]]) as usize;
            let data_offset4 = 12usize;
            Some((start4, device_code4, count4, data_offset4))
        } else {
            None
        };

        // Parse MC3E candidate if buffer is long enough for header
        let mc3 = if data.len() >= 10 {
            let a0 = data[4] as u32;
            let a1 = data[5] as u32;
            let a2 = data[6] as u32;
            let start3 = ((a2 << 16) | (a1 << 8) | a0) as usize;
            let device_code3 = u64::from(data[7]);
            let count3 = u16::from_le_bytes([data[8], data[9]]) as usize;
            let data_offset3 = 10usize;
            Some((start3, device_code3, count3, data_offset3))
        } else {
            None
        };

        // If both are present, pick the one whose expected payload length fits
        // the remaining bytes for write requests. If neither payload check
        // indicates a match, prefer MC4E when fully present, else MC3E.
        match (mc4, mc3) {
            (Some((s4, d4, c4, off4)), Some((s3, d3, c3, off3))) => {
                // compute expected payload bytes. Default to 2 bytes per point (words).
                // For bit write commands (nibble-packed) the payload is ceil(count/2) bytes.
                let is_bit_write =
                    (command == 0x1401 || command == 0x1403) && (sub == 0x0001 || sub == 0x0003);
                #[allow(clippy::manual_div_ceil)]
                let exp4 = if is_bit_write {
                    (c4 + 1) / 2
                } else {
                    c4.saturating_mul(2)
                };
                #[allow(clippy::manual_div_ceil)]
                let exp3 = if is_bit_write {
                    (c3 + 1) / 2
                } else {
                    c3.saturating_mul(2)
                };
                let fits4 = data.len() >= off4 + exp4;
                let fits3 = data.len() >= off3 + exp3;
                if fits4 && !fits3 {
                    Some((s4, d4, c4, off4))
                } else if fits3 && !fits4 {
                    Some((s3, d3, c3, off3))
                } else {
                    // If both or neither fit (read-only or ambiguous), prefer MC4E
                    // when full MC4E header exists; else fall back to MC3E.
                    Some((s4, d4, c4, off4))
                }
            }
            (Some((s4, d4, c4, off4)), None) => Some((s4, d4, c4, off4)),
            (None, Some((s3, d3, c3, off3))) => Some((s3, d3, c3, off3)),
            _ => None,
        }
    };

    // Special-case: Echo test command (0x0619, sub 0x0000)
    // Request: command(2be) subcommand(2be) payload:ascii_hex
    // Response: payload:ascii_hex
    if command == 0x0619 && sub == 0x0000 {
        let payload = &data[4..];
        let len = payload.len();
        if !(1..=960).contains(&len) {
            anyhow::bail!("echo payload length out of range: {}", len);
        }
        // Validate allowed characters: ASCII 0-9, A-F (accept lowercase a-f too)
        for &b in payload {
            let ok = b.is_ascii_digit() || (b'A'..=b'F').contains(&b) || (b'a'..=b'f').contains(&b);
            if !ok {
                anyhow::bail!("echo payload contains invalid character: 0x{:02X}", b);
            }
        }
        return Ok(payload.to_vec());
    }
    // Log monitor timer if present (for subheader+MC3E requests)
    tracing::debug!(
        monitor_timer = req.monitoring_timer,
        "request contains monitor_timer (or default)"
    );
    if let Some(reg) = registry_opt {
        if let Some(spec) = reg.find_by_code_and_sub(command, sub, None) {
            tracing::debug!(
                cmd = spec.id.as_str(),
                cmd_code = spec.command_code,
                "dispatching via CommandRegistry"
            );
            // Attempt to extract simple/typical params from the request bytes
            // Use the centralized extractor so MC3E/MC4E addressing is handled consistently.
            let params = if let Some((start, device_code, count, _data_offset)) =
                read_start_and_device_and_count()
            {
                let mut map = serde_json::Map::new();
                map.insert(
                    "start_addr".to_string(),
                    serde_json::Value::Number(serde_json::Number::from(start as u64)),
                );
                map.insert(
                    "device_code".to_string(),
                    serde_json::Value::Number(serde_json::Number::from(device_code)),
                );
                map.insert(
                    "count".to_string(),
                    serde_json::Value::Number(serde_json::Number::from(count as u64)),
                );
                let mut block = serde_json::Map::new();
                block.insert(
                    "count".to_string(),
                    serde_json::Value::Number(serde_json::Number::from(count as u64)),
                );
                map.insert(
                    "data_blocks".to_string(),
                    serde_json::Value::Array(vec![serde_json::Value::Object(block.clone())]),
                );
                map.insert(
                    "bit_blocks".to_string(),
                    serde_json::Value::Array(vec![serde_json::Value::Object(block)]),
                );
                serde_json::Value::Object(map)
            } else {
                serde_json::Value::Object(serde_json::Map::new())
            };

            // If this is a write command, apply the write into the store first
            use melsec_mc::commands::Command as Cmd;
            if spec.id.is_write() {
                match spec.id {
                    Cmd::WriteWords => {
                        if let Some((start, device_code, count, data_offset)) =
                            read_start_and_device_and_count()
                        {
                            let expected_bytes = count.checked_mul(2).unwrap_or(0);
                            if data.len() >= data_offset + expected_bytes {
                                let mut words: Vec<u16> = Vec::with_capacity(count);
                                for i in 0..count {
                                    let idx = data_offset + i * 2;
                                    let w = u16::from_le_bytes([data[idx], data[idx + 1]]);
                                    words.push(w);
                                }
                                let key_literal =
                                    format!("0x{:02X}", u8::try_from(device_code).unwrap_or(0u8));
                                tracing::info!(key = %key_literal, start = start, words = ?words, "apply write_words to store");
                                let mut s = store.write().await;
                                s.set_words(&key_literal, start, &words);
                                // Per protocol, the logical response payload for write
                                // commands is empty (response_format = []). The transport
                                // builder will prepend the protocol end-code (0x0000).
                                // Return an empty payload here.
                                return Ok(Vec::new());
                            }
                        }
                    }
                    Cmd::WriteBits => {
                        if let Some((start, device_code, count, data_offset)) =
                            read_start_and_device_and_count()
                        {
                            let payload = &data[data_offset..];
                            let mut bools: Vec<u16> = Vec::with_capacity(count);
                            for i in 0..count {
                                let byte_idx = i / 2;
                                if byte_idx >= payload.len() {
                                    bools.push(0);
                                    continue;
                                }
                                let b = payload[byte_idx];
                                let val = if i % 2 == 0 {
                                    (b >> 4) & 0x0F
                                } else {
                                    b & 0x0F
                                };
                                bools.push(if val != 0 { 1u16 } else { 0u16 });
                            }
                            let key_literal =
                                format!("0x{:02X}", u8::try_from(device_code).unwrap_or(0u8));
                            tracing::info!(key = %key_literal, start = start, bits = ?bools, "apply write_bits to store");
                            let mut s = store.write().await;
                            for (i, v) in bools.iter().enumerate() {
                                s.set_words(&key_literal, start + i, &[*v]);
                            }
                            // Logical payload empty; transport will emit end-code.
                            return Ok(Vec::new());
                        }
                    }
                    _ => {}
                }
            }

            // Build response bytes using the spec's response_entries
            if let Ok(resp) = build_response_from_spec(spec, &params, store).await {
                return Ok(resp);
            } else {
                tracing::warn!(
                    cmd = spec.id.as_str(),
                    "registry-driven response build failed, falling back"
                );
            }
        }
    }

    match (command, sub) {
        // read words
        (0x0401, 0x0000) | (0x0401, 0x0002) => {
            if let Some((start, dev_code, count, _data_offset)) = read_start_and_device_and_count()
            {
                let key_literal = format!("0x{:02X}", u8::try_from(dev_code).unwrap_or(0u8));
                let words = {
                    let s = store.read().await;
                    s.get_words(&key_literal, start, count)
                };
                let mut out: Vec<u8> = Vec::with_capacity(words.len() * 2);
                for w in words {
                    out.extend_from_slice(&w.to_le_bytes());
                }
                Ok(out)
            } else {
                Ok(Vec::new())
            }
        }
        // read bits -> respond with nibble-packed bytes (high-first per commands.toml)
        (0x0401, 0x0001) | (0x0401, 0x0003) => {
            if let Some((start, dev_code, count, _data_offset)) = read_start_and_device_and_count()
            {
                let key_literal = format!("0x{:02X}", u8::try_from(dev_code).unwrap_or(0u8));
                let mut bits: Vec<u8> = Vec::with_capacity(count.div_ceil(2));
                for i in (0..count).step_by(2) {
                    let hi = {
                        let s = store.read().await;
                        let v = s.get_words(&key_literal, start + i, 1);
                        if !v.is_empty() && v[0] != 0 {
                            1u8
                        } else {
                            0u8
                        }
                    };
                    let lo = if i + 1 < count {
                        let s = store.read().await;
                        let v = s.get_words(&key_literal, start + i + 1, 1);
                        if !v.is_empty() && v[0] != 0 {
                            1u8
                        } else {
                            0u8
                        }
                    } else {
                        0u8
                    };
                    let byte = (hi << 4) | (lo & 0x0F);
                    bits.push(byte);
                }
                Ok(bits)
            } else {
                Ok(Vec::new())
            }
        }
        // write words: parse payload words and write into store, echo back written words
        (0x1401, 0x0000) | (0x1401, 0x0002) => {
            if let Some((start, dev_code, count, data_offset)) = read_start_and_device_and_count() {
                let expected_bytes = count.checked_mul(2).unwrap_or(0);
                if data.len() < data_offset + expected_bytes {
                    anyhow::bail!("write_words request too short for provided count");
                }
                let mut words: Vec<u16> = Vec::with_capacity(count);
                for i in 0..count {
                    let idx = data_offset + i * 2;
                    let w = u16::from_le_bytes([data[idx], data[idx + 1]]);
                    words.push(w);
                }
                let key_literal = format!("0x{:02X}", u8::try_from(dev_code).unwrap_or(0u8));
                {
                    tracing::info!(key = %key_literal, start = start, words = ?words, "(fallback) apply write_words to store");
                    let mut s = store.write().await;
                    s.set_words(&key_literal, start, &words);
                }
                // Logical payload empty; the response frame builder will include
                // the 0x0000 end-code. Return an empty payload to match
                // `commands.toml`'s `response_format = []`.
                Ok(Vec::new())
            } else {
                Ok(Vec::new())
            }
        }
        // write bits: payload is packed nibbles/bytes starting at index 10
        (0x1401, 0x0001) | (0x1401, 0x0003) => {
            if let Some((start, dev_code, count, data_offset)) = read_start_and_device_and_count() {
                // For nibble-packed write bits, payload offset depends on series
                let payload = &data[data_offset..];
                let mut bools: Vec<u16> = Vec::with_capacity(count);
                for i in 0..count {
                    let byte_idx = i / 2;
                    if byte_idx >= payload.len() {
                        bools.push(0);
                        continue;
                    }
                    let b = payload[byte_idx];
                    let val = if i % 2 == 0 {
                        (b >> 4) & 0x0F
                    } else {
                        b & 0x0F
                    };
                    bools.push(if val != 0 { 1u16 } else { 0u16 });
                }
                let key_literal = format!("0x{:02X}", u8::try_from(dev_code).unwrap_or(0u8));
                {
                    tracing::info!(key = %key_literal, start = start, bits = ?bools, "(fallback) apply write_bits to store");
                    let mut s = store.write().await;
                    for (i, v) in bools.iter().enumerate() {
                        s.set_words(&key_literal, start + i, &[*v]);
                    }
                }
                // Logical payload empty; transport will insert end-code.
                Ok(Vec::new())
            } else {
                Ok(Vec::new())
            }
        }
        _ => Ok(Vec::new()),
    }
}

pub async fn build_response_from_spec(
    spec: &melsec_mc::command_registry::CommandSpec,
    params: &serde_json::Value,
    store: &Arc<RwLock<DeviceMap>>,
) -> Result<Vec<u8>> {
    use melsec_mc::command_registry::ResponseEntry;

    let mut out: Vec<u8> = Vec::new();

    // Helper: extract start_addr/device_code/count for a block entry using the
    // provided BlockTemplate FieldSpecs where available. This prefers explicit
    // object fields inside the block entry, then top-level params, and lastly
    // falls back to heuristic name matching against template field names.
    let get_block_values = |blk: &serde_json::Value,
                            params: &serde_json::Value,
                            template: Option<&melsec_mc::command_registry::BlockTemplate>|
     -> (Option<usize>, Option<u64>, Option<u64>) {
        let mut start_opt: Option<usize> = None;
        let mut device_opt: Option<u64> = None;
        let mut count_opt: Option<u64> = None;

        // helper to extract u64 from possible locations
        let extract_from_obj = |obj: &serde_json::Map<String, serde_json::Value>, key: &str| {
            obj.get(key).and_then(|v| v.as_u64())
        };

        // 1) try explicit block object keys if present
        if let Some(obj) = blk.as_object() {
            if let Some(addr) = extract_from_obj(obj, "start_addr") {
                start_opt = Some(addr as usize);
            }
            // device_code may be a number or a string like "D"/"D100"/"0xA8"
            if let Some(dc) = extract_from_obj(obj, "device_code") {
                device_opt = Some(dc);
            } else if let Some(s) = obj.get("device_code").and_then(|v| v.as_str()) {
                if let Some(dev) = melsec_mc::device::device_by_symbol(s) {
                    device_opt = Some(u64::from(dev.device_code_q()));
                } else if let Ok((dev, _addr)) = melsec_mc::device::parse_device_and_address(s) {
                    device_opt = Some(u64::from(dev.device_code_q()));
                } else if s.starts_with("0x") || s.starts_with("0X") {
                    if let Ok(v) = u64::from_str_radix(&s[2..], 16) {
                        device_opt = Some(v);
                    }
                }
            }
            if let Some(cnt) = extract_from_obj(obj, "count") {
                count_opt = Some(cnt);
            }
        }

        // 2) try top-level params if still missing
        if let Some(obj) = params.as_object() {
            if start_opt.is_none() {
                if let Some(a) = obj.get("start_addr").and_then(|v| v.as_u64()) {
                    start_opt = Some(a as usize);
                }
            }
            if device_opt.is_none() {
                if let Some(d) = obj.get("device_code").and_then(|v| v.as_u64()) {
                    device_opt = Some(d);
                }
            }
            if count_opt.is_none() {
                if let Some(c) = obj.get("count").and_then(|v| v.as_u64()) {
                    count_opt = Some(c);
                }
            }
        }

        // 3) if a template is provided, use its FieldSpec entries to locate
        // names for start/device/count and attempt exact then contains match.
        if let Some(bt) = template {
            for fld in &bt.fields {
                let fname = fld.name.as_str();
                // exact matches
                if start_opt.is_none()
                    && (fname == "start_addr" || fname == "addr" || fname.contains("start"))
                {
                    if let Some(obj) = blk.as_object() {
                        if let Some(v) = obj.get(fname).and_then(|vv| vv.as_u64()) {
                            start_opt = Some(v as usize);
                        }
                    }
                    if start_opt.is_none() {
                        if let Some(obj) = params.as_object() {
                            if let Some(v) = obj.get(fname).and_then(|vv| vv.as_u64()) {
                                start_opt = Some(v as usize);
                            }
                        }
                    }
                }
                if device_opt.is_none() && (fname == "device_code" || fname.contains("device")) {
                    if let Some(obj) = blk.as_object() {
                        if let Some(v) = obj.get(fname).and_then(|vv| vv.as_u64()) {
                            device_opt = Some(v);
                        }
                    }
                    if device_opt.is_none() {
                        if let Some(obj) = params.as_object() {
                            if let Some(v) = obj.get(fname).and_then(|vv| vv.as_u64()) {
                                device_opt = Some(v);
                            }
                        }
                    }
                }
                if count_opt.is_none() && (fname == "count" || fname.contains("count")) {
                    if let Some(obj) = blk.as_object() {
                        if let Some(v) = obj.get(fname).and_then(|vv| vv.as_u64()) {
                            count_opt = Some(v);
                        }
                    }
                    if count_opt.is_none() {
                        if let Some(obj) = params.as_object() {
                            if let Some(v) = obj.get(fname).and_then(|vv| vv.as_u64()) {
                                count_opt = Some(v);
                            }
                        }
                    }
                }
            }
        }

        (start_opt, device_opt, count_opt)
    };

    // cached block arrays map: name -> Vec<Value>
    let mut cached: std::collections::HashMap<String, Vec<serde_json::Value>> =
        std::collections::HashMap::new();
    if let Some(obj) = params.as_object() {
        for (k, v) in obj.iter() {
            if let Some(arr) = v.as_array() {
                cached.insert(k.clone(), arr.clone());
            }
        }
    }

    // Helper: try to find a values array in params using several heuristics:
    // - exact name match
    // - plural/singular variants (foo / foos)
    // - match against request_fields kinds when present
    let find_vals_in_params = |name: &str| -> Option<Vec<serde_json::Value>> {
        if let Some(obj) = params.as_object() {
            // exact
            if let Some(v) = obj.get(name) {
                if let Some(arr) = v.as_array() {
                    return Some(arr.clone());
                }
            }
            // plural/singular fallback
            if let Some(singular) = name.strip_suffix('s') {
                if let Some(v) = obj.get(singular) {
                    if let Some(arr) = v.as_array() {
                        return Some(arr.clone());
                    }
                }
            } else {
                let plural = format!("{}s", name);
                if let Some(v) = obj.get(&plural) {
                    if let Some(arr) = v.as_array() {
                        return Some(arr.clone());
                    }
                }
            }
        }
        None
    };

    for entry in &spec.response_entries {
        match entry {
            ResponseEntry::BlockWords { name, le } => {
                let arr = cached.get(name).cloned().unwrap_or_default();
                let bt_opt = spec
                    .block_templates
                    .iter()
                    .find(|bt| format!("{}s", bt.name) == *name);
                if !arr.is_empty() {
                    for blk in &arr {
                        let (start_opt, dev_opt, count_opt) = get_block_values(blk, params, bt_opt);
                        let count = count_opt.unwrap_or(0u64) as usize;
                        let dev_code = dev_opt.unwrap_or(0u64);
                        let start = start_opt.unwrap_or(0usize);
                        let key_literal =
                            format!("0x{:02X}", u8::try_from(dev_code).unwrap_or(0u8));
                        let words = {
                            let s = store.read().await;
                            s.get_words(&key_literal, start, count)
                        };
                        for w in words {
                            if *le {
                                out.extend_from_slice(&w.to_le_bytes());
                            } else {
                                out.extend_from_slice(&w.to_be_bytes());
                            }
                        }
                    }
                } else if let Some(vals) = find_vals_in_params(name) {
                    let mut le_flag = true;
                    for rf in &spec.request_fields {
                        if rf.name == *name {
                            if let melsec_mc::command_registry::FieldKind::Words { le } = &rf.kind {
                                le_flag = *le;
                            }
                        }
                    }
                    for item in vals {
                        if let Some(n) = item.as_u64() {
                            let w = u16::try_from(n).unwrap_or(0u16);
                            if le_flag {
                                out.extend_from_slice(&w.to_le_bytes());
                            } else {
                                out.extend_from_slice(&w.to_be_bytes());
                            }
                        }
                    }
                }
            }
            ResponseEntry::BlockBitsPacked { name, lsb_first } => {
                let arr = cached.get(name).cloned().unwrap_or_default();
                if !arr.is_empty() {
                    let bt_opt = spec
                        .block_templates
                        .iter()
                        .find(|bt| format!("{}s", bt.name) == *name);
                    for blk in &arr {
                        let (start_opt, dev_opt, count_opt) = get_block_values(blk, params, bt_opt);
                        let count = count_opt.unwrap_or(0u64) as usize;
                        let dev_code = dev_opt.unwrap_or(0u64);
                        let start = start_opt.unwrap_or(0usize);
                        let key_literal =
                            format!("0x{:02X}", u8::try_from(dev_code).unwrap_or(0u8));
                        let mut bits: Vec<bool> = Vec::with_capacity(count);
                        for i in 0..count {
                            let v = {
                                let s = store.read().await;
                                s.get_words(&key_literal, start + i, 1)
                            };
                            bits.push(!v.is_empty() && v[0] != 0);
                        }
                        let mut byte_idx = 0usize;
                        while byte_idx < count {
                            let mut b: u8 = 0;
                            for bit_i in 0..8usize {
                                let idx = byte_idx + bit_i;
                                if idx >= count {
                                    break;
                                }
                                let bit = bits[idx];
                                if *lsb_first {
                                    if bit {
                                        b |= 1u8 << bit_i;
                                    }
                                } else {
                                    let msb_pos = 7 - bit_i;
                                    if bit {
                                        b |= 1u8 << msb_pos;
                                    }
                                }
                            }
                            out.push(b);
                            byte_idx += 8;
                        }
                    }
                } else if let Some(vals) = find_vals_in_params(name) {
                    let mut bits: Vec<bool> = Vec::with_capacity(vals.len());
                    for it in vals {
                        let b = it
                            .as_bool()
                            .unwrap_or_else(|| it.as_u64().is_some_and(|n| n != 0));
                        bits.push(b);
                    }
                    let count = bits.len();
                    let mut byte_idx = 0usize;
                    while byte_idx < count {
                        let mut b: u8 = 0;
                        for bit_i in 0..8usize {
                            let idx = byte_idx + bit_i;
                            if idx >= count {
                                break;
                            }
                            let bit = bits[idx];
                            if *lsb_first {
                                if bit {
                                    b |= 1u8 << bit_i;
                                }
                            } else {
                                let msb_pos = 7 - bit_i;
                                if bit {
                                    b |= 1u8 << msb_pos;
                                }
                            }
                        }
                        out.push(b);
                        byte_idx += 8;
                    }
                }
            }
            ResponseEntry::BlockNibbles { name, high_first } => {
                let arr = cached.get(name).cloned().unwrap_or_default();
                if !arr.is_empty() {
                    let bt_opt = spec
                        .block_templates
                        .iter()
                        .find(|bt| format!("{}s", bt.name) == *name);
                    for blk in &arr {
                        let (start_opt, dev_opt, count_opt) = get_block_values(blk, params, bt_opt);
                        let count = count_opt.unwrap_or(0u64) as usize;
                        let dev_code = dev_opt.unwrap_or(0u64);
                        let start = start_opt.unwrap_or(0usize);
                        let key_literal =
                            format!("0x{:02X}", u8::try_from(dev_code).unwrap_or(0u8));
                        let mut produced = 0usize;
                        while produced < count {
                            let mut high_nibble = 0u8;
                            let mut low_nibble = 0u8;
                            if *high_first {
                                let v = {
                                    let s = store.read().await;
                                    s.get_words(&key_literal, start + produced, 1)
                                };
                                high_nibble = if !v.is_empty() && v[0] != 0 { 1u8 } else { 0u8 };
                                produced += 1;
                                if produced < count {
                                    let v2 = {
                                        let s = store.read().await;
                                        s.get_words(&key_literal, start + produced, 1)
                                    };
                                    low_nibble = if !v2.is_empty() && v2[0] != 0 {
                                        1u8
                                    } else {
                                        0u8
                                    };
                                    produced += 1;
                                }
                            } else {
                                let v = {
                                    let s = store.read().await;
                                    s.get_words(&key_literal, start + produced, 1)
                                };
                                low_nibble = if !v.is_empty() && v[0] != 0 { 1u8 } else { 0u8 };
                                produced += 1;
                                if produced < count {
                                    let v2 = {
                                        let s = store.read().await;
                                        s.get_words(&key_literal, start + produced, 1)
                                    };
                                    high_nibble = if !v2.is_empty() && v2[0] != 0 {
                                        1u8
                                    } else {
                                        0u8
                                    };
                                    produced += 1;
                                }
                            }
                            let byte = (high_nibble << 4) | (low_nibble & 0x0F);
                            out.push(byte);
                        }
                    }
                } else if let Some(vals) = find_vals_in_params(name) {
                    let mut produced = 0usize;
                    let count = vals.len();
                    while produced < count {
                        let mut high = 0u8;
                        let mut low = 0u8;
                        if *high_first {
                            let h = &vals[produced];
                            let hv = h
                                .as_bool()
                                .unwrap_or_else(|| h.as_u64().is_some_and(|n| n != 0));
                            high = if hv { 1u8 } else { 0u8 };
                            produced += 1;
                            if produced < count {
                                let l = &vals[produced];
                                let lv = l
                                    .as_bool()
                                    .unwrap_or_else(|| l.as_u64().is_some_and(|n| n != 0));
                                low = if lv { 1u8 } else { 0u8 };
                                produced += 1;
                            }
                        } else {
                            let l = &vals[produced];
                            let lv = l
                                .as_bool()
                                .unwrap_or_else(|| l.as_u64().is_some_and(|n| n != 0));
                            low = if lv { 1u8 } else { 0u8 };
                            produced += 1;
                            if produced < count {
                                let h = &vals[produced];
                                let hv = h
                                    .as_bool()
                                    .unwrap_or_else(|| h.as_u64().is_some_and(|n| n != 0));
                                high = if hv { 1u8 } else { 0u8 };
                                produced += 1;
                            }
                        }
                        let byte = (high << 4) | (low & 0x0F);
                        out.push(byte);
                    }
                }
            }
            ResponseEntry::AsciiHex { name } => {
                // try to find a string or numeric array in params for this name
                if let Some(v) = params.get(name) {
                    if let Some(s) = v.as_str() {
                        // validate ascii hex bytes
                        for &b in s.as_bytes() {
                            let ok = b.is_ascii_digit()
                                || (b'A'..=b'F').contains(&b)
                                || (b'a'..=b'f').contains(&b);
                            if !ok {
                                anyhow::bail!(
                                    "response ascii_hex contains invalid byte: 0x{:02X}",
                                    b
                                );
                            }
                        }
                        out.extend_from_slice(s.as_bytes());
                    } else if let Some(arr) = v.as_array() {
                        for it in arr {
                            if let Some(n) = it.as_u64() {
                                let b = u8::try_from(n).unwrap_or(0u8);
                                out.push(b);
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    #[tokio::test]
    async fn test_write_bits_mc3e_updates_store() -> Result<(), Box<dyn Error>> {
        // ensure registry etc are initialized
        let _ = melsec_mc::init_defaults();

        let store = Arc::new(RwLock::new(crate::device_map::DeviceMap::new()));

        // prepare params: B0 count=4 bits -> pattern true,false,true,false
        let params = melsec_mc::command_registry::create_write_bits_params(
            "B0",
            &[true, false, true, false],
        );
        let reg =
            melsec_mc::command_registry::CommandRegistry::global().ok_or("registry not set")?;
        let spec = reg
            .get(melsec_mc::commands::Command::WriteBits)
            .ok_or("spec WriteBits not found")?;
        let req_data = spec.build_request(&params, None)?; // MC3E layout
        let mc_req = melsec_mc::request::McRequest::new()
            .with_access_route(melsec_mc::mc_define::AccessRoute::default())
            .try_with_request_data(req_data)?;

        let resp = handle_request_and_apply_store(&store, &mc_req).await?;
        // write commands should return empty logical payload
        assert!(resp.is_empty());

        // verify store updated for B device (bits stored as u16 words)
        let got = {
            let s = store.read().await;
            s.get_words("B0", 0, 4)
        };
        let expected: Vec<u16> = vec![1, 0, 1, 0];
        assert_eq!(got, expected);
        Ok(())
    }

    #[tokio::test]
    async fn test_write_bits_mc4e_updates_store() -> Result<(), Box<dyn Error>> {
        let _ = melsec_mc::init_defaults();

        let store = Arc::new(RwLock::new(crate::device_map::DeviceMap::new()));

        // prepare params: B0 count=6 bits
        let params = melsec_mc::command_registry::create_write_bits_params(
            "B0",
            &[true, false, true, false, true, false],
        );
        let reg =
            melsec_mc::command_registry::CommandRegistry::global().ok_or("registry not set")?;
        let spec = reg
            .get(melsec_mc::commands::Command::WriteBits)
            .ok_or("spec WriteBits not found")?;
        // build with PLCSeries::R to force MC4E layout (4-byte start, 2-byte device_code)
        let req_data = spec.build_request(&params, Some(melsec_mc::plc_series::PLCSeries::R))?;
        let mc_req = melsec_mc::request::McRequest::new()
            .with_access_route(melsec_mc::mc_define::AccessRoute::default())
            .try_with_request_data(req_data)?;

        let resp = handle_request_and_apply_store(&store, &mc_req).await?;
        assert!(resp.is_empty());

        let got = {
            let s = store.read().await;
            s.get_words("B0", 0, 6)
        };
        let expected: Vec<u16> = vec![1, 0, 1, 0, 1, 0];
        assert_eq!(got, expected);
        Ok(())
    }
}
