% commands.toml — schema and usage

This document describes the TOML schema used by `CommandRegistry` to declare request/response command formats.

Overview

- Each file contains one or more `[[command]]` tables.
- A command declares `id`, optional `name`, `command_code`, optional `subcommand`, `request_format`, `response_format`, and optional `block_templates`.

Fields

- `id` (string): internal identifier used to look up the `CommandSpec`.
- `name` (string, optional): human-friendly name.
- `command_code` (integer): 2-byte command code sent in the request (e.g. `0x0120`).
- `subcommand` (integer, optional): 2-byte subcommand.
- `request_format` (array of strings): sequence of header fields to write. Each entry is `name:type` where type can be:
  - `1`, `2`, `3`, ... : fixed bytes (n bytes). Defaults to little-endian for numeric writes.
  - `3le`, `2be`: fixed-width with explicit endianness.
  - `words_le`, `words_be`: a variable-length array of 16-bit words (expects an array in params under that field name).
  - `bytes`: raw byte payload. Accepts either a numeric array (e.g. `[16, 32, 255]`) or a raw string which will be written as-is as bytes. Legacy aliases `rest` and `..` are supported but prefer `bytes` in new definitions.
  - `ascii_hex`: treat the field as an ASCII hex string. The parameter must be a string containing only hex characters `0-9`, `A-F`, or `a-f` (e.g. `"0123AB"`). Each character is written as its ASCII byte value and the parser returns the remaining bytes as a UTF-8 string. Use this for textual hex payloads or echo-like commands.

- `response_format` (array of strings): sequence of response entries describing how to parse the response bytes. Each entry uses the form `key:kind[:option]`.
  - Supported `kind` values:
    - `blocks_words_le` / `blocks_words_be` — parse a sequence of word-blocks; each block's `count` indicates number of 16-bit words. The result will be placed under `key` as an array of arrays (blocks → words).
    - `blocks_bits_packed` — parse packed bits for each block. Optionally append `:msb` to indicate MSB-first packing. Default is LSB-first.

- `block_templates` (array): defines repeated block structures. Each block template includes:
  - `name` (string): base name used to find the block array in params (pluralized to `${name}s` by convention).
  - `repeat_field` (string): which header/count field controls repetition.
  - `fields` (array of field spec strings): same syntax as `request_format` but applied per-block.

Example (read multiple word/bit blocks)

```toml
[[command]]
id = "read_blocks"
name = "ReadMultipleBlocks"
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
```

Example: echo command that accepts/returns ASCII hex payload

```toml
[[command]]
id = "echo"
command_code = 0x0619
request_format = ["command:2be", "subcommand:2be", "payload:ascii_hex"]
response_format = ["payload:ascii_hex"]
device_family = "Any"
```

Usage (Rust)

```rust
// load TOML
let toml = std::fs::read_to_string("examples/commands.example.toml")?;
let reg = toml.parse::<melsec_mc::command_registry::CommandRegistry>()?;
let spec = reg.get("read_blocks").expect("spec");

// params: prepare JSON-like params (serde_json::Value)
let params = serde_json::json!({
  "word_blocks": [ { "start_addr": 100, "device_code": 0xA8, "count": 2 } ],
  "bit_blocks": [ { "start_addr": 300, "device_code": 0x9C, "count": 5 } ]
});

// build raw request bytes for this command
let raw_req = spec.build_request(&params)?;

// or build a full McRequest (subheader + access route handled by caller)
let mc_req = melsec_mc::request::McRequest::from_command_spec(spec, &params, None)?;
let payload = mc_req.build_payload();

// parse a response (simulate bytes or use bytes read from device)
let parsed = spec.parse_response(&params, &response_bytes)?; // serde_json::Value
```

Notes

- The current implementation expects block arrays to be provided under pluralized keys (e.g. `word_blocks` for `word_block`).
- The `response_format` entries are parsed in order; parser consumes bytes sequentially according to the specified entry sequence.
- For advanced bit-mapping (device-specific packing), consider extending the DSL with explicit per-field bit positions.

Define folder (runtime placement)

- At runtime the library will look for `commands.toml` placed under a `define` folder next to the running executable. For example:
  - if the executable is `C:/apps/mytool.exe`, the file should be `C:/apps/define/commands.toml`.
- The example `examples/commands_demo.rs` tries to load this file first and falls back to the bundled example file if not present.

Device overrides

- If you want to supply `device.toml` for runtime overrides of device mapping, place it under the same `define` folder (e.g. `define/device.toml`).
- Note: runtime device overrides are not applied automatically to the static device table generated at build-time. Integrating runtime device overrides is a follow-up task; for now, placing `device.toml` in `define/` is the recommended convention for future use.

---
This file is a quick reference; keep `examples/commands.example.toml` and `examples/commands_demo.rs` as runnable examples.
