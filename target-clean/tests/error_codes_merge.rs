use melsec_mc::error_codes::{code_description, ErrorRegistry};

#[test]
fn register_error_codes_merging_behavior() {
    // Register a TOML containing two codes
    let toml1 = r#"
[[codes]]
code = "0x0000"
description = "正常完了"

[[codes]]
code = "0x0054"
description = "Ethernet設定のデータ形式(ASCII/BINARY)不整合"
"#;

    ErrorRegistry::from_str(toml1)
        .expect("parse toml1")
        .register_or_merge()
        .expect("register first set");
    assert_eq!(code_description(0x0000), Some("正常完了".into()));

    // Register an additional TOML that contains a different code. With merge
    // behavior the previous entries should still be present after the second
    // registration.
    let toml2 = r#"
[[codes]]
code = "0xC00F"
description = "IPアドレスの重複が検出された"
"#;

    ErrorRegistry::from_str(toml2)
        .expect("parse toml2")
        .register_or_merge()
        .expect("register second set");

    // New code must be present
    assert_eq!(
        code_description(0xC00F),
        Some("IPアドレスの重複が検出された".into())
    );
    // Previously registered code must still be present
    assert!(code_description(0x0054).is_some());
}
