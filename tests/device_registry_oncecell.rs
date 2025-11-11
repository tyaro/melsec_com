use melsec_mc::device_registry::DeviceRegistry;

#[test]
fn device_registry_oncecell_set_behavior() {
    let toml1 = r#"
[[device]]
symbol = "X"
code = 0x90
category = "Bit"
"#;

    let reg1 = toml1.parse::<DeviceRegistry>().expect("parse reg1");

    let toml2 = r#"
[[device]]
symbol = "Y"
code = 0x91
category = "Bit"
"#;
    let reg2 = toml2.parse::<DeviceRegistry>().expect("parse reg2");

    // If the global overrides map was not yet set, the first registration should succeed
    // and the second should fail. If it was already set by other tests, the first
    // attempt will return Err and we consider the behavior acceptable (global is set).
    match reg1.register_overrides() {
        Ok(()) => {
            // subsequent registration must fail because OnceCell.set disallows overwrite
            assert!(reg2.register_overrides().is_err());
        }
        Err(_) => {
            // already registered by someone else: ensure attempting to register again also errors
            assert!(reg2.register_overrides().is_err());
        }
    }
}
