use std::env;

fn main() {
    // Generation of device source files has been removed. The crate now uses
    // `src/devices.toml` and `src/device_code.rs` at compile time. If you need
    // to regenerate artifacts for any reason, perform that as an explicit
    // offline step (script) and check generated files into the repository.
    println!("cargo:rerun-if-changed=src/devices.toml");
    // No further action.
    let _ = env::var("REGENERATE_DEVICES");
}
