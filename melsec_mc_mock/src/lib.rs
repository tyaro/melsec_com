//! Lightweight mock PLC server crate (module entry)

pub mod device_map;
pub mod handler;
pub mod server;

pub use server::MockServer;
