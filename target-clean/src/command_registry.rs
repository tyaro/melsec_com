use serde::Deserialize;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::commands::Command;
use crate::device::{device_by_code, DeviceType};
use crate::error::MelsecError;
use crate::plc_series::PLCSeries;
use crate::toml_helpers::extract_line_col_from_msg;

#[derive(Debug, Deserialize)]
struct CommandFile {
    #[serde(rename = "command")]
    pub commands: Vec<CommandSpecRaw>,
}

// Command enum is defined centrally in `src/commands.rs` and imported above.

#[derive(Debug, Deserialize)]
pub struct CommandSpecRaw {
    pub id: Command,
    // name may be provided as a table with localized strings
    pub name: Option<NameField>,
    pub command_code: u16,
    /// subcommand may be a single integer or a table like { Q = 0x0000, R = 0x0200 }
    pub subcommand: Option<toml::Value>,
    pub device_family: Option<String>,
    /// optional per-command limits for total points across blocks
    pub limits: Option<toml::Value>,
    pub min_count: Option<u32>,
    pub max_count: Option<u32>,
    pub request_format: Vec<String>,
    pub response_format: Vec<String>,
    pub block_templates: Option<Vec<BlockTemplateRaw>>,
    pub examples: Option<HashMap<String, String>>,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct NameField {
    pub jp: Option<String>,
    pub en: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BlockTemplateRaw {
    pub name: String,
    pub repeat_field: String,
    pub fields: Vec<String>,
    /// optional per-block device category override: Bit/Word/DoubleWord/Any
    pub device_family: Option<String>,
}

#[derive(Debug, Clone)]
pub enum FieldKind {
    FixedBytes { n: usize, le: bool },
    Words { le: bool },
    Bytes,
    AsciiHex,
}

#[derive(Debug, Clone)]
pub struct FieldSpec {
    pub name: String,
    pub kind: FieldKind,
}

#[derive(Debug)]
pub struct BlockTemplate {
    pub name: String,
    pub repeat_field: String,
    pub fields: Vec<FieldSpec>,
    pub device_family: Option<DeviceFamily>,
}

#[derive(Debug)]
pub struct CommandSpec {
    pub id: Command,
    pub name: Option<NameField>,
    pub command_code: u16,
    pub subcommand: Option<SubcommandSpec>,
    pub device_family: DeviceFamily,
    pub request_fields: Vec<FieldSpec>,
    pub response_format: Vec<String>,
    pub response_entries: Vec<ResponseEntry>,
    pub block_templates: Vec<BlockTemplate>,
    pub limits: Option<Limits>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceFamily {
    Any,
    Bit,
    Word,
    DoubleWord,
}

#[derive(Debug, Clone)]
pub struct Limits {
    pub word_points: Option<u64>,
    pub dword_points: Option<u64>,
    pub bit_points: Option<u64>,
}

#[derive(Debug, Clone)]
pub enum SubcommandSpec {
    Single(u16),
    PerSeries(std::collections::HashMap<String, u16>),
}

#[derive(Debug, Clone)]
pub enum ResponseEntry {
    BlockWords {
        name: String,
        le: bool,
    },
    BlockBitsPacked {
        name: String,
        lsb_first: bool,
    },
    /// Nibble blocks: each point is represented as 4 bits (0..15).
    /// NOTE: current implementation interprets each nibble as a boolean (non-zero -> true)
    /// and returns a boolean array; if numeric nibble values are required, change the
    /// parser to emit numeric JsonValue::Number entries instead.
    /// `high_first=true` means the high nibble (bits 7..4) is the first point in the byte.
    BlockNibbles {
        name: String,
        high_first: bool,
    },
    /// Ascii hex payload: consume remaining bytes and return as UTF-8 string
    AsciiHex {
        name: String,
    },
}

pub struct CommandRegistry {
    by_id: HashMap<Command, CommandSpec>,
}

use once_cell::sync::OnceCell;

pub static GLOBAL_COMMAND_REGISTRY: OnceCell<CommandRegistry> = OnceCell::new();

impl CommandRegistry {
    /// Install this registry as the global (process-wide) registry. Can only be set once.
    pub fn set_global(self) -> Result<(), MelsecError> {
        GLOBAL_COMMAND_REGISTRY
            .set(self)
            .map_err(|_| MelsecError::Protocol("global CommandRegistry already set".into()))
    }

    /// Return a reference to the global registry if one has been registered.
    pub fn global() -> Option<&'static Self> {
        GLOBAL_COMMAND_REGISTRY.get()
    }

    /// Convenience: load `define/commands.toml` next to exe and set as global.
    pub fn load_and_set_global_from_exe_define() -> Result<(), MelsecError> {
        let reg = Self::from_exe_define()?;
        reg.set_global()
    }

    /// Convenience: load `src/commands.toml` compiled into the crate and set as global.
    /// This mirrors `device.rs` behavior of embedding canonical data at compile time.
    pub fn load_and_set_global_from_src() -> Result<(), MelsecError> {
        let s = include_str!("./commands.toml");
        let reg = s.parse::<Self>()?;
        reg.set_global()
    }
}

impl CommandRegistry {
    pub fn from_path(path: &Path) -> Result<Self, MelsecError> {
        let s = fs::read_to_string(path)
            .map_err(|e| MelsecError::Protocol(format!("read toml: {e}")))?;
        s.parse::<Self>()
    }
}

impl CommandRegistry {
    /// Load `commands.toml` from a `define` directory placed next to the running executable.
    /// e.g. if the executable is `.../bin/app.exe`, this will look for `.../bin/define/commands.toml`.
    pub fn from_exe_define() -> Result<Self, MelsecError> {
        let exe = std::env::current_exe()
            .map_err(|e| MelsecError::Protocol(format!("current_exe: {e}")))?;
        let dir = exe.parent().ok_or_else(|| {
            MelsecError::Protocol("cannot determine executable directory".to_string())
        })?;
        let define_path = dir.join("define").join("commands.toml");
        if !define_path.exists() {
            let dp = define_path.display();
            return Err(MelsecError::Protocol(format!(
                "commands.toml not found at {dp}"
            )));
        }
        Self::from_path(&define_path)
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(toml_str: &str) -> Result<Self, MelsecError> {
        let cf: CommandFile = toml::from_str(toml_str).map_err(|e| {
            let s = e.to_string();
            if let Some((line, col)) = extract_line_col_from_msg(&s) {
                MelsecError::Protocol(format!("toml parse error at {line}:{col}: {s}"))
            } else {
                MelsecError::Protocol(format!("toml parse error: {s}"))
            }
        })?;
        let by_id = Self::parse_command_file(cf)?;
        Ok(Self { by_id })
    }

    fn parse_command_file(cf: CommandFile) -> Result<HashMap<Command, CommandSpec>, MelsecError> {
        let mut by_id = HashMap::new();
        for raw in cf.commands {
            let req_fields = raw
                .request_format
                .iter()
                .map(|s| parse_field_spec(s))
                .collect::<Result<Vec<_>, _>>()?;
            let bts = Self::parse_block_templates(raw.block_templates)?;
            // parse response entries from response_format strings
            let resp_entries = Self::parse_response_formats(&raw.response_format)?;

            // Normalize subcommand: could be integer or table { Q = 0x..., R = 0x... }
            let sub_spec = match raw.subcommand {
                Some(toml::Value::Integer(i)) => match u16::try_from(i) {
                    Ok(v) => Some(SubcommandSpec::Single(v)),
                    Err(_) => {
                        return Err(MelsecError::Protocol(format!(
                            "subcommand integer out of range: {i}"
                        )))
                    }
                },
                Some(toml::Value::Table(tbl)) => {
                    let mut map: std::collections::HashMap<String, u16> =
                        std::collections::HashMap::new();
                    for (k, v) in tbl {
                        if let toml::Value::Integer(iv) = v {
                            let iv_u16 = u16::try_from(iv).map_err(|_| {
                                MelsecError::Protocol(format!(
                                    "subcommand table value out of range: {iv}"
                                ))
                            })?;
                            map.insert(k, iv_u16);
                        }
                    }
                    Some(SubcommandSpec::PerSeries(map))
                }
                Some(_) => {
                    return Err(MelsecError::Protocol(
                        "invalid subcommand field type".into(),
                    ))
                }
                None => None,
            };

            // parse device_family string into enum (default Any)
            let device_family = match raw.device_family.as_deref() {
                Some("Bit" | "bit") => DeviceFamily::Bit,
                Some("Word" | "word") => DeviceFamily::Word,
                Some("DoubleWord" | "doubleword") => DeviceFamily::DoubleWord,
                Some(_) | None => DeviceFamily::Any,
            };

            // parse optional limits table: { word_points = N, dword_points = N, bit_points = N }
            let limits = raw.limits.as_ref().and_then(|v| {
                if let toml::Value::Table(ref tbl) = v {
                    // prefer as_integer then try_from to avoid sign-loss casts
                    let wp = tbl
                        .get("word_points")
                        .and_then(toml::Value::as_integer)
                        .and_then(|i| u64::try_from(i).ok());
                    let dp = tbl
                        .get("dword_points")
                        .and_then(toml::Value::as_integer)
                        .and_then(|i| u64::try_from(i).ok());
                    let bp = tbl
                        .get("bit_points")
                        .and_then(toml::Value::as_integer)
                        .and_then(|i| u64::try_from(i).ok());
                    Some(Limits {
                        word_points: wp,
                        dword_points: dp,
                        bit_points: bp,
                    })
                } else {
                    None
                }
            });

            let spec = CommandSpec {
                id: raw.id,
                name: raw.name,
                command_code: raw.command_code,
                subcommand: sub_spec,
                device_family,
                request_fields: req_fields,
                response_format: raw.response_format,
                response_entries: resp_entries,
                block_templates: bts,
                limits,
            };
            by_id.insert(raw.id, spec);
        }
        Ok(by_id)
    }

    fn parse_block_templates(
        bt_raws: Option<Vec<BlockTemplateRaw>>,
    ) -> Result<Vec<BlockTemplate>, MelsecError> {
        let mut bts = Vec::new();
        if let Some(bt_list) = bt_raws {
            for btr in bt_list {
                let fields = btr
                    .fields
                    .iter()
                    .map(|s| parse_field_spec(s))
                    .collect::<Result<Vec<_>, _>>()?;
                // parse optional block-level device_family
                let bf = match btr.device_family.as_deref() {
                    Some("Bit" | "bit") => Some(DeviceFamily::Bit),
                    Some("Word" | "word") => Some(DeviceFamily::Word),
                    Some("DoubleWord" | "doubleword") => Some(DeviceFamily::DoubleWord),
                    Some(_) => Some(DeviceFamily::Any),
                    None => None,
                };
                bts.push(BlockTemplate {
                    name: btr.name,
                    repeat_field: btr.repeat_field,
                    fields,
                    device_family: bf,
                });
            }
        }
        Ok(bts)
    }

    fn parse_response_formats(formats: &Vec<String>) -> Result<Vec<ResponseEntry>, MelsecError> {
        let mut resp_entries: Vec<ResponseEntry> = Vec::new();
        for rf in formats {
            // expected forms: "word_blocks:blocks_words_le" or "bit_blocks:blocks_bits_packed[:lsb|:msb]"
            let parts: Vec<&str> = rf.split(':').collect();
            if parts.len() < 2 {
                return Err(MelsecError::Protocol(format!(
                    "invalid response_format: {rf}"
                )));
            }
            let name = parts[0].to_string();
            let kind = parts[1];
            match kind {
                "blocks_words_le" => {
                    resp_entries.push(ResponseEntry::BlockWords { name, le: true })
                }
                "blocks_words_be" => {
                    resp_entries.push(ResponseEntry::BlockWords { name, le: false })
                }
                "blocks_bits_packed" => {
                    // optional param for lsb/msb
                    let mut lsb = true;
                    if parts.len() >= 3 {
                        let p = parts[2];
                        if p == "msb" {
                            lsb = false;
                        }
                    }
                    resp_entries.push(ResponseEntry::BlockBitsPacked {
                        name,
                        lsb_first: lsb,
                    });
                }
                "blocks_nibbles" => {
                    // optional param for high/low nibble order
                    let mut high_first = true;
                    if parts.len() >= 3 {
                        let p = parts[2];
                        if p == "low" {
                            high_first = false;
                        }
                    }
                    resp_entries.push(ResponseEntry::BlockNibbles { name, high_first });
                }
                "ascii_hex" => {
                    // ascii_hex: treat response as raw ASCII hex string
                    resp_entries.push(ResponseEntry::AsciiHex { name });
                }
                other => {
                    return Err(MelsecError::Protocol(format!(
                        "unsupported response kind: {other}"
                    )))
                }
            }
        }
        Ok(resp_entries)
    }

    /// Validate the TOML string for command definitions. Performs structural and semantic checks
    /// (unique ids, field spec parsing, block repeat fields present, response_format names refer
    /// to known blocks/fields).
    pub fn validate_str(toml_str: &str) -> Result<(), MelsecError> {
        let cf: CommandFile = toml::from_str(toml_str).map_err(|e| {
            let s = e.to_string();
            if let Some((line, col)) = extract_line_col_from_msg(&s) {
                MelsecError::Protocol(format!("toml parse error at {line}:{col}: {s}"))
            } else {
                MelsecError::Protocol(format!("toml parse error: {s}"))
            }
        })?;
        let mut ids = std::collections::HashSet::new();
        for cmd in &cf.commands {
            if !ids.insert(cmd.id) {
                return Err(MelsecError::Protocol(format!(
                    "duplicate command id: {id}",
                    id = cmd.id.as_str()
                )));
            }
            // validate request_format entries
            let mut req_names = Vec::new();
            for rf in &cmd.request_format {
                let fs = parse_field_spec(rf)?;
                req_names.push(fs.name);
            }
            // validate block templates
            if let Some(bts) = cmd.block_templates.as_ref() {
                for bt in bts {
                    // repeat_field must exist in req_names
                    if !req_names.iter().any(|n| n == &bt.repeat_field) {
                        return Err(MelsecError::Protocol(format!("block template '{}' repeat_field '{}' not found in request_format for command {}", bt.name, bt.repeat_field, cmd.id.as_str())));
                    }
                    // validate optional device_family string on block template if present
                    if let Some(df) = bt.device_family.as_ref() {
                        match df.as_str() {
                            "Bit" | "bit" | "Word" | "word" | "DoubleWord" | "doubleword"
                            | "Any" | "any" => {}
                            _ => {
                                return Err(MelsecError::Protocol(format!(
                                "invalid device_family '{}' on block template '{}' for command {}",
                                df,
                                bt.name,
                                cmd.id.as_str()
                            )))
                            }
                        }
                    }
                    // validate block field specs
                    for f in &bt.fields {
                        let _ = parse_field_spec(f)?;
                    }
                }
            }
            // validate response_format entries
            for rf in &cmd.response_format {
                let parts: Vec<&str> = rf.split(':').collect();
                if parts.len() < 2 {
                    return Err(MelsecError::Protocol(format!(
                        "invalid response_format entry: {rf}"
                    )));
                }
                let name = parts[0];
                // allow response name to be either a request field or a pluralized block name
                let mut ok = req_names.iter().any(|n| n == name);
                if !ok {
                    if let Some(bts) = cmd.block_templates.as_ref() {
                        for bt in bts {
                            if format!("{}s", bt.name) == name {
                                ok = true;
                                break;
                            }
                        }
                    }
                }
                if !ok {
                    return Err(MelsecError::Protocol(format!("response_format name '{name}' not found in request fields or block templates for command {cmd}", cmd = cmd.id.as_str())));
                }
            }
        }
        Ok(())
    }

    /// Get a command spec by `Command` enum. Prefer this typed API to avoid
    /// repeated string parsing at call sites.
    #[must_use]
    pub fn get(&self, id: Command) -> Option<&CommandSpec> {
        self.by_id.get(&id)
    }

    /// Convenience: lookup by string id (parses into `Command`).
    #[must_use]
    pub fn get_by_str(&self, id: &str) -> Option<&CommandSpec> {
        id.parse::<Command>()
            .map_or(None, |cmd| self.by_id.get(&cmd))
    }

    /// Find a command spec by numeric command code and subcommand.
    /// This resolves per-series subcommand mappings when present.
    pub fn find_by_code_and_sub(
        &self,
        command_code: u16,
        subcommand: u16,
        plc_series: Option<PLCSeries>,
    ) -> Option<&CommandSpec> {
        for spec in self.by_id.values() {
            if spec.command_code != command_code {
                continue;
            }
            // resolve spec.subcommand to an effective u16 (default 0)
            let effective_sub = match &spec.subcommand {
                Some(SubcommandSpec::Single(v)) => *v,
                Some(SubcommandSpec::PerSeries(map)) => {
                    // prefer series-specific mapping if provided, otherwise first value
                    if let Some(series) = plc_series {
                        let key = match series {
                            PLCSeries::Q => "Q",
                            PLCSeries::R => "R",
                        };
                        map.get(key)
                            .copied()
                            .or_else(|| map.values().next().copied())
                            .unwrap_or(0)
                    } else {
                        map.get("Q")
                            .copied()
                            .or_else(|| map.values().next().copied())
                            .unwrap_or(0)
                    }
                }
                None => 0u16,
            };
            if effective_sub == subcommand {
                return Some(spec);
            }
        }
        None
    }
}

impl std::str::FromStr for CommandRegistry {
    type Err = MelsecError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Delegate to the inherent `from_str` implementation. Writing
        // `CommandRegistry::from_str` makes the intent explicit and avoids
        // confusion with the trait method resolution.
        CommandRegistry::from_str(s)
    }
}

fn parse_field_spec(s: &str) -> Result<FieldSpec, MelsecError> {
    // format: name:type[:param]
    let parts: Vec<&str> = s.splitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(MelsecError::Protocol(format!("invalid field spec: {s}")));
    }
    let name = parts[0].to_string();
    let typ = parts[1];
    // handle common patterns
    // legacy alias: "rest" or ".." map to explicit "bytes" kind
    if typ == "rest" || typ == ".." {
        return Ok(FieldSpec {
            name,
            kind: FieldKind::Bytes,
        });
    }
    if typ == "bytes" {
        return Ok(FieldSpec {
            name,
            kind: FieldKind::Bytes,
        });
    }
    if typ.starts_with("words") {
        let le = typ.ends_with("le");
        return Ok(FieldSpec {
            name,
            kind: FieldKind::Words { le },
        });
    }
    // e.g., 3le, 2be, 1
    if typ.ends_with("le") || typ.ends_with("be") {
        let le = typ.ends_with("le");
        let nstr = typ.trim_end_matches("le").trim_end_matches("be");
        let n: usize = nstr
            .parse()
            .map_err(|_| MelsecError::Protocol(format!("invalid byte width: {typ}")))?;
        return Ok(FieldSpec {
            name,
            kind: FieldKind::FixedBytes { n, le },
        });
    }
    // ascii_hex payload: treat as a sequence of ASCII hex characters (one byte per char)
    if typ == "ascii_hex" {
        return Ok(FieldSpec {
            name,
            kind: FieldKind::AsciiHex,
        });
    }
    // numeric single bytes like "1", "2"
    if let Ok(n) = typ.parse::<usize>() {
        return Ok(FieldSpec {
            name,
            kind: FieldKind::FixedBytes { n, le: true },
        });
    }
    Err(MelsecError::Protocol(format!(
        "unsupported field type: {typ}"
    )))
}

// extract_line_col_from_msg was moved to `src/toml_helpers.rs` and is imported above.

impl CommandSpec {
    // Helper: read "count" from a block descriptor JSON object and convert to usize.
    fn read_block_count(block: &JsonValue, name: &str) -> Result<usize, MelsecError> {
        let count_u64 = block
            .get("count")
            .and_then(JsonValue::as_u64)
            .ok_or_else(|| MelsecError::Protocol(format!("missing count in block {name}")))?;
        let count = usize::try_from(count_u64)
            .map_err(|_| MelsecError::Protocol(format!("count out of range for block {name}")))?;
        Ok(count)
    }

    // Helper: ensure there are at least `needed` bytes available from `offset` into `bytes`.
    fn ensure_bytes_available(
        bytes: &[u8],
        offset: usize,
        needed: usize,
        kind: &str,
    ) -> Result<(), MelsecError> {
        if offset + needed > bytes.len() {
            return Err(MelsecError::Protocol(format!(
                "response too short for {kind}"
            )));
        }
        Ok(())
    }

    // Helper: insert or append an array value into the result map under `key`.
    fn push_array_in_result_map(
        result_map: &mut serde_json::Map<String, JsonValue>,
        key: &str,
        value: JsonValue,
    ) {
        if let Some(JsonValue::Array(arr)) = result_map.get_mut(key) {
            arr.push(value);
            return;
        }
        result_map.insert(key.to_string(), JsonValue::Array(vec![value]));
    }

    /// Return a display name for this command in the requested language.
    /// `lang` supports "jp" (or "ja") and "en". If the requested language
    /// is not present, falls back to the other one if available.
    #[must_use]
    pub fn display_name(&self, lang: &str) -> Option<&str> {
        let name = self.name.as_ref()?;
        match lang {
            "jp" | "ja" => name
                .jp
                .as_ref()
                .map_or(name.en.as_deref(), |s| Some(s.as_str())),
            "en" => name
                .en
                .as_ref()
                .map_or(name.jp.as_deref(), |s| Some(s.as_str())),
            _ => {
                // default: prefer english then japanese
                name.en
                    .as_ref()
                    .map_or(name.jp.as_deref(), |s| Some(s.as_str()))
            }
        }
    }

    /// Build request bytes from params. params is a JSON object where arrays or objects
    /// are used for block templates (e.g. "`word_blocks`": [{...}, ...])
    /// `plc_series` optionally indicates the target PLC series (Q/R) so per-series
    /// subcommand mappings can be resolved. If `None`, a reasonable default is chosen
    /// (prefer Q if present in a per-series map).
    pub fn build_request(
        &self,
        params: &JsonValue,
        plc_series: Option<PLCSeries>,
    ) -> Result<Vec<u8>, MelsecError> {
        let mut out: Vec<u8> = Vec::new();
        // Validate and write request fields
        self.write_request_fields(params, plc_series, &mut out)?;
        // Append blocks
        self.append_blocks(params, plc_series, &mut out)?;
        Ok(out)
    }

    fn write_request_fields(
        &self,
        params: &JsonValue,
        plc_series: Option<PLCSeries>,
        out: &mut Vec<u8>,
    ) -> Result<(), MelsecError> {
        // top-level device_code check uses command-level family
        if let Some(dc) = params.get("device_code").and_then(JsonValue::as_u64) {
            self.validate_code_for_family(dc, self.device_family)?;
        }
        // check block entries using block-level family when present otherwise fall back to command-level
        for bt in &self.block_templates {
            let key = format!("{}s", bt.name);
            if let Some(arr) = params.get(&key).and_then(|v| v.as_array()) {
                for entry in arr {
                    if let Some(obj) = entry.as_object() {
                        if let Some(dc) = obj.get("device_code").and_then(JsonValue::as_u64) {
                            let family = bt.device_family.unwrap_or(self.device_family);
                            self.validate_code_for_family(dc, family)?;
                        }
                    }
                }
            }
        }

        // write fields sequentially by delegating to small helpers to reduce cognitive complexity
        for f in &self.request_fields {
            match &f.kind {
                FieldKind::FixedBytes { .. } => {
                    self.write_fixed_bytes_field(f, params, plc_series, out)?
                }
                FieldKind::Words { .. } => Self::write_words_field(f, params, out)?,
                FieldKind::Bytes => Self::write_bytes_field(f, params, out)?,
                FieldKind::AsciiHex => Self::write_ascii_hex_field(f, params, out)?,
            }
        }
        Ok(())
    }

    fn write_fixed_bytes_field(
        &self,
        f: &FieldSpec,
        params: &JsonValue,
        plc_series: Option<PLCSeries>,
        out: &mut Vec<u8>,
    ) -> Result<(), MelsecError> {
        // read numeric value for this field
        let v = self.get_num_field(&f.name, params, plc_series)?;
        // Special-case endianness/width for certain protocol fields
        let (n, le) = match &f.kind {
            FieldKind::FixedBytes { n, le } => (*n, *le),
            _ => unreachable!("called write_fixed_bytes_field for non-fixed field"),
        };
        let (effective_n, effective_le) = if f.name == "command" || f.name == "subcommand" {
            (n, true)
        } else if f.name == "device_code" {
            match plc_series {
                Some(PLCSeries::R) => (2usize, true),
                _ => (1usize, true),
            }
        } else if f.name == "start_addr" {
            match plc_series {
                Some(PLCSeries::R) => (4usize, true),
                _ => (n, le),
            }
        } else {
            (n, le)
        };
        write_n_bytes(out, effective_n, v, effective_le);
        Ok(())
    }

    fn write_words_field(
        f: &FieldSpec,
        params: &JsonValue,
        out: &mut Vec<u8>,
    ) -> Result<(), MelsecError> {
        let le = match &f.kind {
            FieldKind::Words { le } => *le,
            _ => unreachable!("called write_words_field for non-words field"),
        };
        if let Some(arr) = params.get(&f.name).and_then(JsonValue::as_array) {
            for item in arr {
                if let Some(num) = item.as_u64() {
                    let n = u16::try_from(num).map_err(|_| {
                        MelsecError::Protocol(format!(
                            "word array item out of range for u16: {num}"
                        ))
                    })?;
                    if le {
                        out.extend_from_slice(&n.to_le_bytes());
                    } else {
                        out.extend_from_slice(&n.to_be_bytes());
                    }
                } else {
                    return Err(MelsecError::Protocol(format!(
                        "word array item not number: {name}",
                        name = f.name
                    )));
                }
            }
        } else {
            return Err(MelsecError::Protocol(format!(
                "expected array for words field: {name}",
                name = f.name
            )));
        }
        Ok(())
    }

    fn validate_code_for_family(
        &self,
        code_num: u64,
        family: DeviceFamily,
    ) -> Result<(), MelsecError> {
        let code = u8::try_from(code_num)
            .map_err(|_| MelsecError::Protocol(format!("device code out of range: {code_num}")))?;
        if let Some(dev) = device_by_code(code) {
            match family {
                DeviceFamily::Any => {}
                DeviceFamily::Bit => {
                    if dev.category != DeviceType::Bit {
                        return Err(MelsecError::Protocol(format!(
                            "command '{}' requires a bit device but device code 0x{:02X} is {:?}",
                            self.id.as_str(),
                            code,
                            dev.category
                        )));
                    }
                }
                DeviceFamily::Word => {
                    if !(dev.category == DeviceType::Word || dev.category == DeviceType::DoubleWord)
                    {
                        return Err(MelsecError::Protocol(format!(
                            "command '{}' requires a word device but device code 0x{:02X} is {:?}",
                            self.id.as_str(),
                            code,
                            dev.category
                        )));
                    }
                }
                DeviceFamily::DoubleWord => {
                    if dev.category != DeviceType::DoubleWord {
                        return Err(MelsecError::Protocol(format!("command '{}' requires a double-word device but device code 0x{:02X} is {:?}", self.id.as_str(), code, dev.category)));
                    }
                }
            }
        }
        Ok(())
    }

    fn get_num_field(
        &self,
        name: &str,
        params: &JsonValue,
        plc_series: Option<PLCSeries>,
    ) -> Result<u64, MelsecError> {
        // special-case command/subcommand
        if name == "command" {
            return Ok(u64::from(self.command_code));
        }
        if name == "subcommand" {
            // Resolve subcommand depending on the provided PLC series (if any) or use
            // Single/default value.
            let sub = match &self.subcommand {
                Some(SubcommandSpec::Single(v)) => Some(*v),
                Some(SubcommandSpec::PerSeries(map)) => plc_series.map_or_else(
                    || {
                        map.get("Q")
                            .map_or_else(|| map.values().next().copied(), |v| Some(*v))
                    },
                    |series| {
                        let key = match series {
                            PLCSeries::Q => "Q",
                            PLCSeries::R => "R",
                        };
                        map.get(key)
                            .map_or_else(|| map.values().next().copied(), |v| Some(*v))
                    },
                ),
                None => None,
            };
            return Ok(u64::from(sub.unwrap_or(0)));
        }
        // word_block_count/bit_block_count computed from params
        if name == "word_block_count" {
            if let Some(arr) = params.get("word_blocks").and_then(|v| v.as_array()) {
                return Ok(arr.len() as u64);
            }
            return Ok(0);
        }
        if name == "bit_block_count" {
            if let Some(arr) = params.get("bit_blocks").and_then(|v| v.as_array()) {
                return Ok(arr.len() as u64);
            }
            return Ok(0);
        }
        if let Some(v) = params.get(name) {
            if v.is_number() {
                if let Some(i) = v.as_u64() {
                    return Ok(i);
                }
                if let Some(i) = v.as_i64() {
                    if i >= 0 {
                        let uv = u64::try_from(i).map_err(|_| {
                            MelsecError::Protocol(format!("numeric param out of range: {i}"))
                        })?;
                        return Ok(uv);
                    }
                }
            }
        }
        Err(MelsecError::Protocol(format!(
            "missing numeric param: {name}"
        )))
    }

    fn append_blocks(
        &self,
        params: &JsonValue,
        plc_series: Option<PLCSeries>,
        out: &mut Vec<u8>,
    ) -> Result<(), MelsecError> {
        // Split into validate + emit to reduce cognitive complexity
        self.validate_block_limits(params)?;
        self.emit_blocks(params, plc_series, out)
    }

    fn validate_block_limits(&self, params: &JsonValue) -> Result<(), MelsecError> {
        if let Some(lim) = &self.limits {
            let mut total_word_points: u64 = 0;
            let mut total_dword_points_sum: u64 = 0;
            let mut total_bits: u64 = 0;
            for bt in &self.block_templates {
                let key = format!("{}s", bt.name);
                if let Some(arr) = params.get(&key).and_then(JsonValue::as_array) {
                    for entry in arr {
                        if let Some(obj) = entry.as_object() {
                            if let Some(dc) = obj.get("device_code").and_then(JsonValue::as_u64) {
                                if let Some(count) = obj.get("count").and_then(JsonValue::as_u64) {
                                    if let Ok(dc_u8) = u8::try_from(dc) {
                                        if let Some(dev) = device_by_code(dc_u8) {
                                            match dev.category {
                                                DeviceType::Word => total_word_points += count,
                                                DeviceType::DoubleWord => {
                                                    total_dword_points_sum += count
                                                }
                                                DeviceType::Bit => total_bits += count,
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if let Some(wp) = lim.word_points {
                if total_word_points > wp {
                    return Err(MelsecError::Protocol(format!(
                        "command '{}' exceeds word_points limit: {} > {}",
                        self.id.as_str(),
                        total_word_points,
                        wp
                    )));
                }
            }
            if let Some(dp) = lim.dword_points {
                if total_dword_points_sum > dp {
                    return Err(MelsecError::Protocol(format!(
                        "command '{}' exceeds dword_points limit: {} > {}",
                        self.id.as_str(),
                        total_dword_points_sum,
                        dp
                    )));
                }
            }
            if let Some(bp) = lim.bit_points {
                if total_bits > bp {
                    return Err(MelsecError::Protocol(format!(
                        "command '{}' exceeds bit_points limit: {} > {}",
                        self.id.as_str(),
                        total_bits,
                        bp
                    )));
                }
            }
        }
        Ok(())
    }

    fn emit_blocks(
        &self,
        params: &JsonValue,
        plc_series: Option<PLCSeries>,
        out: &mut Vec<u8>,
    ) -> Result<(), MelsecError> {
        for bt in &self.block_templates {
            Self::emit_block_template(bt, params, plc_series, out)?;
        }
        Ok(())
    }

    fn emit_block_template(
        bt: &BlockTemplate,
        params: &JsonValue,
        plc_series: Option<PLCSeries>,
        out: &mut Vec<u8>,
    ) -> Result<(), MelsecError> {
        let key = format!("{}s", bt.name);
        let arr = params
            .get(&key)
            .and_then(JsonValue::as_array)
            .ok_or_else(|| MelsecError::Protocol(format!("missing block array: {key}")))?;
        for entry in arr {
            if !entry.is_object() {
                return Err(MelsecError::Protocol("block entry must be object".into()));
            }
            let map = entry
                .as_object()
                .ok_or_else(|| MelsecError::Protocol("block entry must be object".into()))?;
            for fld in &bt.fields {
                match &fld.kind {
                    FieldKind::FixedBytes { .. } => {
                        Self::emit_field_fixed_bytes(fld, map, plc_series, out)?
                    }
                    FieldKind::Words { .. } => Self::emit_field_words(fld, map, out)?,
                    FieldKind::Bytes => {}
                    FieldKind::AsciiHex => {}
                }
            }
        }
        Ok(())
    }

    fn emit_field_fixed_bytes(
        fld: &FieldSpec,
        map: &serde_json::Map<String, JsonValue>,
        plc_series: Option<PLCSeries>,
        out: &mut Vec<u8>,
    ) -> Result<(), MelsecError> {
        let val = map
            .get(&fld.name)
            .and_then(JsonValue::as_u64)
            .ok_or_else(|| {
                MelsecError::Protocol(format!("missing block field {name}", name = fld.name))
            })?;
        let (n, le) = match &fld.kind {
            FieldKind::FixedBytes { n, le } => (*n, *le),
            _ => unreachable!(),
        };
        let (effective_n, effective_le) = if fld.name == "device_code" {
            match plc_series {
                Some(PLCSeries::R) => (2usize, true),
                _ => (1usize, true),
            }
        } else if fld.name == "start_addr" {
            match plc_series {
                Some(PLCSeries::R) => (4usize, true),
                _ => (n, le),
            }
        } else {
            (n, le)
        };
        write_n_bytes(out, effective_n, val, effective_le);
        Ok(())
    }

    fn emit_field_words(
        fld: &FieldSpec,
        map: &serde_json::Map<String, JsonValue>,
        out: &mut Vec<u8>,
    ) -> Result<(), MelsecError> {
        let le = match &fld.kind {
            FieldKind::Words { le } => *le,
            _ => unreachable!(),
        };
        if let Some(warr) = map.get(&fld.name).and_then(JsonValue::as_array) {
            for it in warr {
                let val_u64 = it
                    .as_u64()
                    .ok_or_else(|| MelsecError::Protocol("word array item not number".into()))?;
                let val_u16 = u16::try_from(val_u64).map_err(|_| {
                    MelsecError::Protocol(format!(
                        "word array item out of range for u16: {val_u64}"
                    ))
                })?;
                if le {
                    out.extend_from_slice(&val_u16.to_le_bytes());
                } else {
                    out.extend_from_slice(&val_u16.to_be_bytes());
                }
            }
        } else {
            let val_u64 = map
                .get(&fld.name)
                .and_then(JsonValue::as_u64)
                .ok_or_else(|| {
                    MelsecError::Protocol(format!("missing field {name}", name = fld.name))
                })?;
            let val_u16 = u16::try_from(val_u64).map_err(|_| {
                MelsecError::Protocol(format!(
                    "field {name} out of range for u16: {val_u64}",
                    name = fld.name
                ))
            })?;
            if le {
                out.extend_from_slice(&val_u16.to_le_bytes());
            } else {
                out.extend_from_slice(&val_u16.to_be_bytes());
            }
        }
        Ok(())
    }

    /// Write a numeric byte array or raw-string-as-bytes for `Bytes` fields.
    fn write_bytes_field(
        f: &FieldSpec,
        params: &JsonValue,
        out: &mut Vec<u8>,
    ) -> Result<(), MelsecError> {
        // numeric array of byte values
        if let Some(arr) = params.get(&f.name).and_then(JsonValue::as_array) {
            for it in arr {
                let bv = it.as_u64().ok_or_else(|| {
                    MelsecError::Protocol(format!("payload item not number: {name}", name = f.name))
                })?;
                let b = u8::try_from(bv).map_err(|_| {
                    MelsecError::Protocol(format!("payload item out of range for u8: {bv}"))
                })?;
                out.push(b);
            }
            return Ok(());
        }
        // fallback: string -> treat as raw bytes (no ascii-hex validation)
        if let Some(s) = params.get(&f.name).and_then(|v| v.as_str()) {
            out.extend_from_slice(s.as_bytes());
            return Ok(());
        }
        Ok(())
    }

    /// Write an ascii-hex string (validate hex chars) or numeric-array for `AsciiHex` fields.
    fn write_ascii_hex_field(
        f: &FieldSpec,
        params: &JsonValue,
        out: &mut Vec<u8>,
    ) -> Result<(), MelsecError> {
        // string form: must be ASCII hex
        if let Some(s) = params.get(&f.name).and_then(|v| v.as_str()) {
            let bytes = s.as_bytes();
            for &b in bytes {
                let ok =
                    b.is_ascii_digit() || (b'A'..=b'F').contains(&b) || (b'a'..=b'f').contains(&b);
                if !ok {
                    return Err(MelsecError::Protocol(format!(
                        "payload string contains invalid ascii_hex byte: 0x{:02X}",
                        b
                    )));
                }
                out.push(b);
            }
            return Ok(());
        }
        // numeric array fallback
        if let Some(arr) = params.get(&f.name).and_then(JsonValue::as_array) {
            for it in arr {
                let bv = it.as_u64().ok_or_else(|| {
                    MelsecError::Protocol(format!("payload item not number: {name}", name = f.name))
                })?;
                let b = u8::try_from(bv).map_err(|_| {
                    MelsecError::Protocol(format!("payload item out of range for u8: {bv}"))
                })?;
                out.push(b);
            }
            return Ok(());
        }
        Ok(())
    }

    fn parse_response_entries(
        &self,
        params: &JsonValue,
        bytes: &[u8],
        offset: &mut usize,
        cached_arrays: &HashMap<String, Vec<JsonValue>>,
        result_map: &mut serde_json::Map<String, JsonValue>,
    ) -> Result<(), MelsecError> {
        for entry in &self.response_entries {
            match entry {
                ResponseEntry::BlockWords { name, le } => Self::parse_block_words_entry(
                    name,
                    *le,
                    params,
                    bytes,
                    offset,
                    cached_arrays,
                    result_map,
                )?,
                ResponseEntry::BlockBitsPacked { name, lsb_first } => {
                    Self::parse_block_bits_packed_entry(
                        name,
                        *lsb_first,
                        params,
                        bytes,
                        offset,
                        cached_arrays,
                        result_map,
                    )?
                }
                ResponseEntry::BlockNibbles { name, high_first } => {
                    Self::parse_block_nibbles_entry(
                        name,
                        *high_first,
                        params,
                        bytes,
                        offset,
                        cached_arrays,
                        result_map,
                    )?
                }
                ResponseEntry::AsciiHex { name } => {
                    // consume remaining bytes as an ASCII hex string
                    if *offset > bytes.len() {
                        return Err(MelsecError::Protocol("response offset beyond end".into()));
                    }
                    let rem = &bytes[*offset..];
                    // validate content
                    for &b in rem {
                        let ok = b.is_ascii_digit()
                            || (b'A'..=b'F').contains(&b)
                            || (b'a'..=b'f').contains(&b);
                        if !ok {
                            return Err(MelsecError::Protocol(format!(
                                "response ascii_hex contains invalid byte: 0x{:02X}",
                                b
                            )));
                        }
                    }
                    let s = std::str::from_utf8(rem).map_err(|_| {
                        MelsecError::Protocol("response ascii_hex not valid utf8".into())
                    })?;
                    result_map.insert(name.to_string(), JsonValue::String(s.to_string()));
                    *offset = bytes.len();
                }
            }
        }
        Ok(())
    }

    fn parse_block_words_entry(
        name: &str,
        le: bool,
        params: &JsonValue,
        bytes: &[u8],
        offset: &mut usize,
        cached_arrays: &HashMap<String, Vec<JsonValue>>,
        result_map: &mut serde_json::Map<String, JsonValue>,
    ) -> Result<(), MelsecError> {
        let arr = cached_arrays.get(name).cloned().unwrap_or_default();
        let mut out_blocks: Vec<JsonValue> = Vec::new();
        for block in &arr {
            let count = Self::read_block_count(block, name)?;
            let bytes_needed = count
                .checked_mul(2)
                .ok_or_else(|| MelsecError::Protocol("count too large".into()))?;
            Self::ensure_bytes_available(bytes, *offset, bytes_needed, "word block")?;
            let mut words = Vec::new();
            for i in 0..count {
                let b0 = bytes[*offset + i * 2];
                let b1 = bytes[*offset + i * 2 + 1];
                let val = if le {
                    u16::from_le_bytes([b0, b1])
                } else {
                    u16::from_be_bytes([b0, b1])
                };
                let val32 = u32::from(val);
                words.push(JsonValue::from(val32));
            }
            *offset += bytes_needed;
            out_blocks.push(JsonValue::Array(words));

            let dc_opt = block
                .as_object()
                .and_then(|o| o.get("device_code").and_then(JsonValue::as_u64))
                .or_else(|| params.get("device_code").and_then(JsonValue::as_u64));
            if let Some(dc) = dc_opt {
                if let Ok(dc_u8) = u8::try_from(dc) {
                    if let Some(dev) = crate::device::device_by_code(dc_u8) {
                        use crate::device::DeviceType;
                        if dev.category == DeviceType::Bit && count == 1 {
                            if let Some(JsonValue::Array(last_block)) = out_blocks.last() {
                                if last_block.len() == 1 {
                                    if let Some(numv) = last_block[0].as_u64() {
                                        let w = u16::try_from(numv).unwrap_or(0);
                                        let mut bits_arr: Vec<JsonValue> = Vec::with_capacity(16);
                                        for i in 0..16 {
                                            let bit = ((w >> i) & 0x01) != 0;
                                            bits_arr.push(JsonValue::Bool(bit));
                                        }
                                        Self::push_array_in_result_map(
                                            result_map,
                                            "bit_blocks",
                                            JsonValue::Array(bits_arr),
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        result_map.insert(name.to_string(), JsonValue::Array(out_blocks));
        Ok(())
    }

    fn parse_block_bits_packed_entry(
        name: &str,
        lsb_first: bool,
        _params: &JsonValue,
        bytes: &[u8],
        offset: &mut usize,
        cached_arrays: &HashMap<String, Vec<JsonValue>>,
        result_map: &mut serde_json::Map<String, JsonValue>,
    ) -> Result<(), MelsecError> {
        let arr = cached_arrays.get(name).cloned().unwrap_or_default();
        let mut out_blocks: Vec<JsonValue> = Vec::new();
        for block in &arr {
            let count = Self::read_block_count(block, name)?;
            let bytes_needed = count.div_ceil(8);
            Self::ensure_bytes_available(bytes, *offset, bytes_needed, "bit block")?;
            let mut bits: Vec<JsonValue> = Vec::new();
            for i in 0..count {
                let byte_idx = *offset + (i / 8);
                let bit_idx = i % 8;
                let b = bytes[byte_idx];
                let bit = if lsb_first {
                    ((b >> bit_idx) & 0x01) != 0
                } else {
                    let msb_idx = 7 - bit_idx;
                    ((b >> msb_idx) & 0x01) != 0
                };
                bits.push(JsonValue::Bool(bit));
            }
            *offset += bytes_needed;
            out_blocks.push(JsonValue::Array(bits));
        }
        result_map.insert(name.to_string(), JsonValue::Array(out_blocks));
        Ok(())
    }

    fn parse_block_nibbles_entry(
        name: &str,
        high_first: bool,
        _params: &JsonValue,
        bytes: &[u8],
        offset: &mut usize,
        cached_arrays: &HashMap<String, Vec<JsonValue>>,
        result_map: &mut serde_json::Map<String, JsonValue>,
    ) -> Result<(), MelsecError> {
        let arr = cached_arrays.get(name).cloned().unwrap_or_default();
        let mut out_blocks: Vec<JsonValue> = Vec::new();
        for block in &arr {
            let count = Self::read_block_count(block, name)?;
            let bytes_needed = count.div_ceil(2); // 2 nibbles per byte
            Self::ensure_bytes_available(bytes, *offset, bytes_needed, "nibble block")?;
            let mut nibbles: Vec<JsonValue> = Vec::new();
            let mut produced = 0usize;
            for i in 0..bytes_needed {
                let b = bytes[*offset + i];
                if high_first {
                    if produced < count {
                        let high = (b >> 4) & 0x0F;
                        nibbles.push(JsonValue::Bool(high != 0));
                        produced += 1;
                    }
                    if produced < count {
                        let low = b & 0x0F;
                        nibbles.push(JsonValue::Bool(low != 0));
                        produced += 1;
                    }
                } else {
                    if produced < count {
                        let low = b & 0x0F;
                        nibbles.push(JsonValue::Bool(low != 0));
                        produced += 1;
                    }
                    if produced < count {
                        let high = (b >> 4) & 0x0F;
                        nibbles.push(JsonValue::Bool(high != 0));
                        produced += 1;
                    }
                }
            }
            *offset += bytes_needed;
            out_blocks.push(JsonValue::Array(nibbles));
        }
        result_map.insert(name.to_string(), JsonValue::Array(out_blocks));
        Ok(())
    }

    /// Parse response bytes given the same params used when building the request.
    /// Returns a JSON value with keys like "`word_blocks`" and "`bit_blocks`" containing arrays.
    pub fn parse_response(
        &self,
        params: &JsonValue,
        bytes: &[u8],
    ) -> Result<JsonValue, MelsecError> {
        let mut offset = 0usize;
        // Prepare a mapping of param arrays for quick lookup (e.g. "word_blocks" -> Vec)
        let mut cached_arrays: HashMap<String, Vec<JsonValue>> = HashMap::new();
        if let Some(obj) = params.as_object() {
            for (k, v) in obj {
                if let Some(arr) = v.as_array() {
                    cached_arrays.insert(k.clone(), arr.clone());
                }
            }
        }

        let mut result_map = serde_json::Map::new();

        self.parse_response_entries(params, bytes, &mut offset, &cached_arrays, &mut result_map)?;

        Ok(JsonValue::Object(result_map))
    }
}

fn write_n_bytes(out: &mut Vec<u8>, n: usize, mut v: u64, le: bool) {
    // write least-significant n bytes in given endianness
    // Use a small stack buffer for typical n (<=8) to avoid heap allocation.
    if n <= 8 {
        let mut buf = [0u8; 8];
        for slot in buf.iter_mut().take(n) {
            *slot = (v & 0xFF) as u8;
            v >>= 8;
        }
        if !le {
            buf[..n].reverse();
        }
        out.extend_from_slice(&buf[..n]);
    } else {
        // fallback for unusually large n: allocate on heap
        let mut tmp = Vec::with_capacity(n);
        for _ in 0..n {
            tmp.push((v & 0xFF) as u8);
            v >>= 8;
        }
        if !le {
            tmp.reverse();
        }
        out.extend_from_slice(&tmp);
    }
}

// helpers
// build_requestの引数に渡すJSON作成をサポートするヘルパーメソッド群
#[must_use]
pub fn create_read_words_params(device: &str, count: u16) -> JsonValue {
    use crate::device::{device_by_symbol, parse_device_and_address};
    let mut params = serde_json::Map::new();
    // parse device like "D500" into (Device, addr)
    // Accept either "D0" or just "D". If only the symbol is provided, assume start_addr=0.
    match parse_device_and_address(device) {
        Ok((_dev, addr)) => {
            params.insert(
                "start_addr".to_string(),
                JsonValue::Number(serde_json::Number::from(u64::from(addr))),
            );
        }
        Err(_) => {
            // Treat alphabetic-only input like "D" as symbol-only and default address 0
            if device.chars().all(|c| c.is_ascii_alphabetic()) {
                params.insert(
                    "start_addr".to_string(),
                    JsonValue::Number(serde_json::Number::from(0u64)),
                );
            }
        }
    }
    // device code (numeric)
    match parse_device_and_address(device) {
        Ok((dev, _)) => {
            params.insert(
                "device_code".to_string(),
                JsonValue::Number(serde_json::Number::from(u64::from(dev.device_code_q()))),
            );
        }
        Err(_) => {
            if device.chars().all(|c| c.is_ascii_alphabetic()) {
                if let Some(dev) = device_by_symbol(&device.to_uppercase()) {
                    params.insert(
                        "device_code".to_string(),
                        JsonValue::Number(serde_json::Number::from(u64::from(dev.device_code_q()))),
                    );
                }
            }
        }
    }
    params.insert(
        "count".to_string(),
        JsonValue::Number(serde_json::Number::from(count)),
    );
    // Provide response block descriptor so parse_response can split incoming bytes.
    // commands.toml for read_words uses "data_blocks" as response name.
    let mut block = serde_json::Map::new();
    block.insert(
        "count".to_string(),
        JsonValue::Number(serde_json::Number::from(u64::from(count))),
    );
    params.insert(
        "data_blocks".to_string(),
        JsonValue::Array(vec![JsonValue::Object(block)]),
    );
    JsonValue::Object(params)
}
#[must_use]
pub fn create_read_bits_params(device: &str, count: u16) -> JsonValue {
    use crate::device::{device_by_symbol, parse_device_and_address};
    let mut params = serde_json::Map::new();
    match parse_device_and_address(device) {
        Ok((_dev, addr)) => {
            params.insert(
                "start_addr".to_string(),
                JsonValue::Number(serde_json::Number::from(u64::from(addr))),
            );
        }
        Err(_) => {
            if device.chars().all(|c| c.is_ascii_alphabetic()) {
                params.insert(
                    "start_addr".to_string(),
                    JsonValue::Number(serde_json::Number::from(0u64)),
                );
            }
        }
    }
    if let Ok((dev, _)) = parse_device_and_address(device) {
        params.insert(
            "device_code".to_string(),
            JsonValue::Number(serde_json::Number::from(u64::from(dev.device_code_q()))),
        );
    } else if device.chars().all(|c| c.is_ascii_alphabetic()) {
        if let Some(dev) = device_by_symbol(&device.to_uppercase()) {
            params.insert(
                "device_code".to_string(),
                JsonValue::Number(serde_json::Number::from(u64::from(dev.device_code_q()))),
            );
        }
    }
    params.insert(
        "count".to_string(),
        JsonValue::Number(serde_json::Number::from(count)),
    );
    // Provide response block descriptor for bit blocks
    let mut block = serde_json::Map::new();
    block.insert(
        "count".to_string(),
        JsonValue::Number(serde_json::Number::from(u64::from(count))),
    );
    // Insert both 'bit_blocks' and 'data_blocks' so parse_response can find the
    // descriptor regardless of command response naming (some commands use
    // 'data_blocks' for nibble responses).
    params.insert(
        "bit_blocks".to_string(),
        JsonValue::Array(vec![JsonValue::Object(block.clone())]),
    );
    params.insert(
        "data_blocks".to_string(),
        JsonValue::Array(vec![JsonValue::Object(block)]),
    );
    JsonValue::Object(params)
}
#[must_use]
pub fn create_write_words_params(device: &str, values: &[u16]) -> JsonValue {
    use crate::device::{device_by_symbol, parse_device_and_address};
    let mut params = serde_json::Map::new();
    match parse_device_and_address(device) {
        Ok((_dev, addr)) => {
            params.insert(
                "start_addr".to_string(),
                JsonValue::Number(serde_json::Number::from(u64::from(addr))),
            );
        }
        Err(_) => {
            if device.chars().all(|c| c.is_ascii_alphabetic()) {
                params.insert(
                    "start_addr".to_string(),
                    JsonValue::Number(serde_json::Number::from(0u64)),
                );
            }
        }
    }
    if let Ok((dev, _)) = parse_device_and_address(device) {
        params.insert(
            "device_code".to_string(),
            JsonValue::Number(serde_json::Number::from(u64::from(dev.device_code_q()))),
        );
    } else if device.chars().all(|c| c.is_ascii_alphabetic()) {
        if let Some(dev) = device_by_symbol(&device.to_uppercase()) {
            params.insert(
                "device_code".to_string(),
                JsonValue::Number(serde_json::Number::from(u64::from(dev.device_code_q()))),
            );
        }
    }
    params.insert(
        "count".to_string(),
        JsonValue::Number(serde_json::Number::from(values.len() as u64)),
    );
    let vals_json: Vec<JsonValue> = values
        .iter()
        .map(|&v| JsonValue::Number(serde_json::Number::from(v)))
        .collect();
    // 'data' matches request_format key in commands.toml for write_words
    params.insert("data".to_string(), JsonValue::Array(vals_json));
    JsonValue::Object(params)
}
#[must_use]
pub fn create_write_bits_params(device: &str, values: &[bool]) -> JsonValue {
    use crate::device::{device_by_symbol, parse_device_and_address};
    let mut params = serde_json::Map::new();
    match parse_device_and_address(device) {
        Ok((_dev, addr)) => {
            params.insert(
                "start_addr".to_string(),
                JsonValue::Number(serde_json::Number::from(u64::from(addr))),
            );
        }
        Err(_) => {
            if device.chars().all(|c| c.is_ascii_alphabetic()) {
                params.insert(
                    "start_addr".to_string(),
                    JsonValue::Number(serde_json::Number::from(0u64)),
                );
            }
        }
    }
    if let Ok((dev, _)) = parse_device_and_address(device) {
        params.insert(
            "device_code".to_string(),
            JsonValue::Number(serde_json::Number::from(u64::from(dev.device_code_q()))),
        );
    } else if device.chars().all(|c| c.is_ascii_alphabetic()) {
        if let Some(dev) = device_by_symbol(&device.to_uppercase()) {
            params.insert(
                "device_code".to_string(),
                JsonValue::Number(serde_json::Number::from(u64::from(dev.device_code_q()))),
            );
        }
    }
    params.insert(
        "count".to_string(),
        JsonValue::Number(serde_json::Number::from(values.len())),
    );
    // Pack booleans into 4-bit nibbles high-first per point.
    // Each pair of points becomes one byte: first point -> high nibble, second -> low nibble.
    // This matches observed PLC behavior (e.g. values [true,false,true,...] -> bytes 0x10, 0x10...).
    let mut payload_bytes: Vec<u8> = Vec::new();
    let mut i = 0usize;
    while i < values.len() {
        let high = u8::from(values[i]);
        let low = if i + 1 < values.len() {
            u8::from(values[i + 1])
        } else {
            0u8
        };
        let byte = (high << 4) | (low & 0x0F);
        payload_bytes.push(byte);
        i += 2;
    }
    let payload_json: Vec<JsonValue> = payload_bytes
        .iter()
        .map(|b| JsonValue::Number(serde_json::Number::from(*b)))
        .collect();
    params.insert("payload".to_string(), JsonValue::Array(payload_json));
    JsonValue::Object(params)
}

// For raw 4-bit (nibble) payloads, build params manually or use `create_write_bits_params`
// which packs booleans into high-first 4-bit nibbles. The explicit helper for packing
// numeric nibble arrays was removed because it's not used by the public client APIs.

// (Removed) test helper `unpack_4bit_payload` — this was used only by unit tests and
// has been removed together with its dedicated unit test per project hygiene request.

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const EXAMPLE: &str = r#"
[[command]]
id = "read_blocks"
    name = { jp = "複数ブロック一括読出し", en = "ReadMultipleBlocks" }
command_code = 0x0120
subcommand = 0x0000
request_format = ["command:2be", "subcommand:2be", "word_block_count:1", "bit_block_count:1"]
response_format = ["word_blocks:blocks_words_le", "bit_blocks:blocks_bits_packed"]

[[command.block_templates]]
name = "word_block"
repeat_field = "word_block_count"
fields = ["start_addr:3le", "device_code:1", "count:2le"]

[[command.block_templates]]
name = "bit_block"
repeat_field = "bit_block_count"
fields = ["start_addr:3le", "device_code:1", "count:2le"]
"#;

    #[test]
    fn test_build_and_parse_block_request() {
        let registry = EXAMPLE.parse::<CommandRegistry>().expect("load");
        let spec = registry.get(Command::ReadBlocks).expect("spec");

        // params: two word blocks (counts 2 and 4), one bit block (count 5)
        let params = json!({
            "word_blocks": [
                { "start_addr": 100u64, "device_code": 0xA8u64, "count": 2u64 },
                { "start_addr": 200u64, "device_code": 0xA8u64, "count": 4u64 }
            ],
            "bit_blocks": [
                { "start_addr": 300u64, "device_code": 0x9Cu64, "count": 5u64 }
            ]
        });

        let request = spec.build_request(&params, None).expect("build req");
        // header: command(2le)=0x0120, sub(2le)=0x0000, word_block_count=2, bit_block_count=1
        // command/subcommand are little-endian per commands.toml semantics
        assert_eq!(request[0], 0x20);
        assert_eq!(request[1], 0x01);
        assert_eq!(request[2], 0x00);
        assert_eq!(request[3], 0x00);
        assert_eq!(request[4], 2u8);
        assert_eq!(request[5], 1u8);

        // craft a response: words first
        let mut resp: Vec<u8> = Vec::new();
        // word block 0: count=2 words -> provide 2 words: 0x1122, 0x3344 little-endian per parser assumption
        resp.extend_from_slice(&[0x22, 0x11, 0x44, 0x33]);
        // word block 1: count=4 words
        resp.extend_from_slice(&[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]);
        // bit block: count=5 -> 1 byte, bitpacked LSB-first: e.g. bits 10110 -> 0b01101 = 0x0D
        resp.push(0x0D);

        let parsed = spec.parse_response(&params, &resp).expect("parse");
        // check structure
        let wb = parsed
            .get("word_blocks")
            .and_then(|v| v.as_array())
            .expect("word_blocks array missing");
        assert_eq!(wb.len(), 2, "expected two word blocks");
        let first_words = wb[0].as_array().expect("first word block is not an array");
        assert_eq!(first_words.len(), 2, "expected two words in first block");
        let first_word_val = first_words[0].as_u64().expect("first word is not a number");
        assert_eq!(first_word_val, 0x1122);
        let bb = parsed
            .get("bit_blocks")
            .and_then(|v| v.as_array())
            .expect("bit_blocks array missing");
        assert_eq!(bb.len(), 1, "expected one bit block");
        let bits = bb[0].as_array().expect("bit block is not an array");
        assert_eq!(bits.len(), 5, "expected 5 bits in bit block");
        // bit 0 is LSB of 0x0D = 1
        assert!(bits[0].as_bool().expect("bit value not boolean"));
    }

    #[test]
    fn test_parse_nibbles_response() {
        let toml = r#"
[[command]]
id = "read_bits"
command_code = 0x0401
subcommand = 0x0001
request_format = ["command:2be", "subcommand:2be", "start_addr:3le", "device_code:1", "count:2le"]
response_format = ["data_blocks:blocks_nibbles:high"]
"#;
        let registry = toml.parse::<CommandRegistry>().expect("load toml");
        let spec = registry.get(Command::ReadBits).expect("spec");

        // params: single nibble block count=4
        let params = serde_json::json!({"data_blocks": [{"count": 4u64}], "start_addr": 0u64, "device_code": 0x90u64, "count": 4u64});

        // craft response bytes: two bytes, both 0x10 to represent [1,0,1,0]
        let resp_bytes: Vec<u8> = vec![0x10u8, 0x10u8];

        let parsed = spec
            .parse_response(&params, &resp_bytes)
            .expect("parse response");
        let db = parsed
            .get("data_blocks")
            .and_then(|v| v.as_array())
            .expect("data_blocks missing");
        let arr = db[0].as_array().expect("data_blocks[0] is not an array");
        assert_eq!(arr.len(), 4, "expected 4 nibble points");
        assert!(arr[0].as_bool().expect("arr[0] not boolean"));
        assert!(!arr[1].as_bool().expect("arr[1] not boolean"));
        assert!(arr[2].as_bool().expect("arr[2] not boolean"));
        assert!(!arr[3].as_bool().expect("arr[3] not boolean"));
    }

    #[test]
    fn test_parse_nibbles_response_observed() {
        // Using same command definition as production for read_bits (high nibble first)
        let toml = r#"
[[command]]
id = "read_bits"
command_code = 0x0401
subcommand = 0x0001
request_format = ["command:2be", "subcommand:2be", "start_addr:3le", "device_code:1", "count:2le"]
response_format = ["data_blocks:blocks_nibbles:high"]
"#;
        let registry = toml.parse::<CommandRegistry>().expect("load toml");
        let spec = registry.get(Command::ReadBits).expect("spec");

        // params: single nibble block count=11
        let params = serde_json::json!({"data_blocks": [{"count": 11u64}], "start_addr": 20u64, "device_code": 0x90u64, "count": 11u64});

        // craft response bytes: six bytes of 0x10 -> high nibble=1, low=0 repeating
        let resp_bytes: Vec<u8> = vec![0x10u8; 6];

        let parsed = spec
            .parse_response(&params, &resp_bytes)
            .expect("parse response");
        let db = parsed
            .get("data_blocks")
            .and_then(|v| v.as_array())
            .expect("data_blocks missing");
        let arr = db[0].as_array().expect("data_blocks[0] is not an array");
        assert_eq!(arr.len(), 11, "expected 11 nibble points");
        for (i, val) in arr.iter().enumerate().take(11) {
            let expected = i % 2 == 0;
            let got = val
                .as_bool()
                .unwrap_or_else(|| panic!("value at index {} not boolean", i));
            assert_eq!(got, expected, "mismatch at index {i}");
        }
    }

    #[test]
    fn test_create_write_4bit_params() {
        // low-first packer removed; use the high-first helper to construct payloads or
        // write booleans via McClient::write_bits in normal cases.
    }

    // Test for unpack_4bit_payload removed along with the helper it exercised.

    #[test]
    fn test_build_request_device_category_restriction() {
        // build a write_bits command spec and attempt to target a word device (D=0xA8)
        let toml = r#"
[[command]]
id = "write_bits"
command_code = 0x1401
subcommand = 0x0001
device_family = "Bit"
request_format = ["command:2be", "subcommand:2be", "start_addr:3le", "device_code:1", "count:2le", "payload:rest"]
response_format = ["data_blocks:blocks_bits_packed"]
"#;
        let registry = toml.parse::<CommandRegistry>().expect("load toml");
        let spec = registry.get(Command::WriteBits).expect("spec");

        // params with device_code = 0xA8 (D) which is a Word device in devices.toml
        let params = serde_json::json!({"start_addr": 0u64, "device_code": 0xA8u64, "count": 2u64, "payload": [1u64, 0u64]});
        let res = spec.build_request(&params, None);
        assert!(
            res.is_err(),
            "expected build_request to reject a word device for write_bits"
        );
    }

    #[test]
    fn test_block_level_device_family_mismatch() {
        // command-level Any, but block template declares Bit; passing a Word device_code should be rejected
        let toml = r#"
[[command]]
id = "write_blocks"
command_code = 0x1406
subcommand = 0x0000
request_format = ["command:2be", "subcommand:2be", "word_block_count:1", "bit_block_count:1"]
response_format = ["word_blocks:blocks_words_le", "bit_blocks:blocks_bits_packed"]

[[command.block_templates]]
name = "bit_block"
repeat_field = "bit_block_count"
fields = ["start_addr:3le", "device_code:1", "count:2le"]
device_family = "Bit"
"#;
        let registry = toml.parse::<CommandRegistry>().expect("load toml");
        let spec = registry.get(Command::WriteBlocks).expect("spec");

        // provide a bit_block entry that targets device_code 0xA8 (D = Word)
        let params = serde_json::json!({
            "bit_blocks": [ { "start_addr": 0u64, "device_code": 0xA8u64, "count": 1u64 } ],
        });
        let res = spec.build_request(&params, None);
        assert!(
            res.is_err(),
            "expected build_request to reject block device mismatch (bit block -> word device)"
        );
    }

    #[test]
    fn test_block_level_device_family_override_ok() {
        // command-level Bit, but block template declares Word and we pass a Word device -> should succeed
        let toml = r#"
[[command]]
id = "write_blocks"
command_code = 0x1406
subcommand = 0x0000
device_family = "Bit"
request_format = ["command:2be", "subcommand:2be", "word_block_count:1", "bit_block_count:1"]
response_format = ["word_blocks:blocks_words_le", "bit_blocks:blocks_bits_packed"]

[[command.block_templates]]
name = "word_block"
repeat_field = "word_block_count"
fields = ["start_addr:3le", "device_code:1", "count:2le"]
device_family = "Word"
"#;
        let registry = toml.parse::<CommandRegistry>().expect("load toml");
        let spec = registry.get(Command::WriteBlocks).expect("spec");

        // provide a word_block entry that targets device_code 0xA8 (D = Word)
        let params = serde_json::json!({
            "word_blocks": [ { "start_addr": 0u64, "device_code": 0xA8u64, "count": 1u64 } ],
        });
        let res = spec.build_request(&params, None);
        assert!(
            res.is_ok(),
            "expected build_request to accept block device override to Word"
        );
    }

    #[test]
    fn test_read_words_accepts_bit_device() {
        let toml = r#"
[[command]]
id = "read_words"
command_code = 0x0401
subcommand = 0x0000
device_family = "Any"
request_format = ["command:2be", "subcommand:2be", "start_addr:3le", "device_code:1", "count:2le"]
response_format = ["data_blocks:blocks_words_le"]
"#;
        let registry = toml.parse::<CommandRegistry>().expect("load toml");
        let spec = registry.get(Command::ReadWords).expect("spec");

        // use helper to create params for bit device M0
        let params = create_read_words_params("M0", 2);
        let res = spec.build_request(&params, None);
        assert!(res.is_ok(), "read_words should accept bit device (M)");
    }

    #[test]
    fn test_write_words_accepts_bit_device() {
        let toml = r#"
[[command]]
id = "write_words"
command_code = 0x1401
subcommand = 0x0000
device_family = "Any"
request_format = ["command:2be", "subcommand:2be", "start_addr:3le", "device_code:1", "count:2le", "data:words_le"]
response_format = ["data_blocks:blocks_words_le"]
"#;
        let registry = toml.parse::<CommandRegistry>().expect("load toml");
        let spec = registry.get(Command::WriteWords).expect("spec");

        // create write params targeting M0 (bit device) but providing word data
        let params = create_write_words_params("M0", &[0x1234u16, 0x5678u16]);
        let res = spec.build_request(&params, None);
        assert!(res.is_ok(), "write_words should accept bit device (M)");
    }

    #[test]
    fn test_word_device_does_not_create_bit_blocks() {
        // Ensure that when the target device is a Word device (e.g. D / device_code 0xA8),
        // parse_response does not inject a 'bit_blocks' entry.
        let toml = r#"
[[command]]
id = "read_words"
command_code = 0x0401
subcommand = 0x0000
request_format = ["command:2be", "subcommand:2be", "start_addr:3le", "device_code:1", "count:2le"]
response_format = ["data_blocks:blocks_words_le"]
"#;
        let registry = CommandRegistry::from_str(toml).expect("load toml");
        let spec = registry.get(Command::ReadWords).expect("spec");

        // params: single word block targeting device code 0xA8 (D = Word)
        let params = serde_json::json!({"data_blocks": [{"count": 1u64, "device_code": 0xA8u64}], "start_addr": 0u64, "device_code": 0xA8u64, "count": 1u64});

        // craft response: one little-endian word 0x1234
        let resp: Vec<u8> = vec![0x34u8, 0x12u8];

        let parsed = spec.parse_response(&params, &resp).expect("parse");
        // data_blocks should exist
        assert!(
            parsed.get("data_blocks").is_some(),
            "expected data_blocks present"
        );
        // bit_blocks must NOT be injected for a word device
        assert!(
            parsed.get("bit_blocks").is_none(),
            "bit_blocks should not be present for word device responses"
        );
    }
}
