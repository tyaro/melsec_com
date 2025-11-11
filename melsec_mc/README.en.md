# melsec_mc — English summary

Lightweight Rust library for Mitsubishi Electric PLCs using the MC protocol (Ethernet / MC4E compatible).

Key features

- Async TCP transport built on Tokio
- Low-level MC frame parsing and helpers
- A reusable `McClient` for common read/write operations
- Typed read/write support with `FromWords` / `ToWords`

Install

Add to your `Cargo.toml`:

```toml
[dependencies]
melsec_mc = "0.4.13"
```

Quick example

```rust
use melsec_mc::{McClient, ConnectionTarget};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let target = ConnectionTarget::direct("192.168.1.40", 4020);
    let client = McClient::new().with_target(target).with_mc_format(melsec_mc::mc_define::McFrameFormat::MC4E);

    let bits = client.read_bits("M100", 10).await?;
    println!("bits: {:?}", bits);

    let words = client.read_words("D1000", 2).await?;
    println!("words: {:?}", words);

    Ok(())
}
```

Notes

- For publishing, run `cargo publish` (requires crates.io credentials and possibly 2FA).
- See the main `README.md` for more detailed Japanese documentation and examples.
