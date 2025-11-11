// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // call into the library-runner implemented in src/lib.rs
    melsec_mc_gui_lib::run()
}
