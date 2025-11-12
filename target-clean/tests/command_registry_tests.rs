use melsec_mc::command_registry::CommandRegistry;
use melsec_mc::commands::Command;
use serde_json::json;

const SIMPLE_READ_WORDS: &str = r#"
[[command]]
id = "read_words"
name = { jp = "ワード単位の一括読出し", en = "ReadWords" }
command_code = 0x0401
subcommand = 0x0000
request_format = ["command:2be", "subcommand:2be", "start_addr:3le", "device_code:1", "count:2le"]
response_format = ["count:blocks_words_le"]
"#;

#[test]
fn load_simple_commands_and_build_read_words() {
    let registry = SIMPLE_READ_WORDS
        .parse::<CommandRegistry>()
        .expect("load simple toml");
    let spec = registry
        .get(Command::ReadWords)
        .expect("read_words spec present");

    // Build a minimal request params matching the request_format in the simple TOML
    let params = json!({
        "start_addr": 0u64,
        "device_code": 0xA8u64,
        "count": 4u64
    });

    let request_bytes = spec.build_request(&params, None).expect("build req");
    // command_code is 0x0401 -> little-endian 2 bytes per commands.toml semantics
    assert_eq!(request_bytes[0], 0x01);
    assert_eq!(request_bytes[1], 0x04);
    assert!(request_bytes.len() >= 6);
}
