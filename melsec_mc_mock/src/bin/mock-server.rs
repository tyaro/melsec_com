use clap::Parser;

#[derive(Parser)]
struct Opts {
    /// listen address, e.g. 127.0.0.1:5000
    #[clap(long, default_value = "127.0.0.1:5000")]
    listen: String,
    /// optional admin HTTP bind address, e.g. 127.0.0.1:8000
    // admin API removed: management via UDP/TCP or programmatic APIs only
    /// optional UDP listen address, e.g. 127.0.0.1:5001
    #[clap(long)]
    udp: Option<String>,
    /// TIM_AWAIT timeout in milliseconds (overrides MELSEC_MOCK_TIM_AWAIT_MS env var)
    #[clap(long)]
    tim_await_ms: Option<u64>,
    /// Optional device assignment TOML file (format: `[devices] SYMBOL = <points>`)
    #[clap(long)]
    device_assignment: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    tracing_subscriber::fmt::init();

    let server = melsec_mc_mock::MockServer::new_with_assignment(opts.device_assignment.as_deref());
    // If tim_await_ms provided via CLI, set environment variable so server picks it up
    if let Some(ms) = opts.tim_await_ms {
        std::env::set_var("MELSEC_MOCK_TIM_AWAIT_MS", ms.to_string());
    }
    tracing::info!(listen = %opts.listen, "starting mock server");

    // admin API support removed from CLI

    // If udp address provided, start UDP listener in background
    if let Some(udp_bind) = opts.udp.clone() {
        let udp_srv = server.clone();
        tracing::info!(udp = %udp_bind, "starting UDP listener");
        tokio::spawn(async move {
            if let Err(e) = udp_srv.run_udp_listener(&udp_bind).await {
                tracing::error!(%e, "udp listener failed");
            }
        });
    }

    // Run the MC listener (blocks until error)
    server.run_listener(&opts.listen).await?;
    Ok(())
}
