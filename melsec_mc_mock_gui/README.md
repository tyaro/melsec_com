# melsec_mc_mock

Lightweight mock PLC server for local testing of `melsec_mc` clients.

This crate provides a programmatic DeviceMap and a placeholder TCP listener. It is intended
to be extended to wire incoming MC payloads to the existing `melsec_mc` parser/response
builders and serve realistic protocol replies.

Usage (development):

```
cargo run -p melsec_mc_mock --bin mock-server -- --listen 127.0.0.1:5000
```

Admin HTTP API

The admin HTTP API has been removed from the mock server binary. Use the
programmatic `MockServer` API (`set_words` / `get_words`) or send MC frames
directly over UDP/TCP to interact with the server. Update any scripts or CI
that previously used the `--admin` flag to the new approach.
