//! mx4d: owns the MX Master 4 connection and runs the enabled modules.

mod config;

use std::io::IsTerminal;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use mx4_core::haptics::WAVEFORMS;
use mx4_core::{Device, DeviceConfig, Haptics};
use tokio::signal::unix::{SignalKind, signal};
use tracing::info;

#[derive(Parser)]
#[command(version, about = "MX Master 4 daemon for Linux")]
struct Args {
    /// Play every waveform once and exit
    #[arg(long)]
    test: bool,
    /// Debug logging
    #[arg(short, long)]
    verbose: bool,
    /// Config file [default: ~/.config/mx4/config.toml]
    #[arg(long)]
    config: Option<PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    tracing_subscriber::fmt()
        .with_max_level(if args.verbose {
            tracing::Level::DEBUG
        } else {
            tracing::Level::INFO
        })
        .with_target(false)
        .with_ansi(std::io::stdout().is_terminal())
        .init();

    let cfg = config::load(&args.config.unwrap_or_else(config::default_path))?;
    #[cfg(feature = "haptic")]
    cfg.haptic.validate()?;

    let device = Device::new(DeviceConfig::new(
        cfg.device.path.clone(),
        cfg.device.number,
    ));
    let haptics = Haptics::new(device, cfg.device.feature_index);

    if args.test {
        run_test(&haptics);
        return Ok(());
    }

    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(run(haptics, cfg))
}

fn run_test(haptics: &Haptics) {
    println!("MX Master 4 Haptic Test — playing all waveforms");
    let mut waveforms = WAVEFORMS.to_vec();
    waveforms.sort_by_key(|&(_, i)| i);
    for (name, index) in waveforms {
        print!("  [{index:#04x}] {name}... ");
        println!(
            "{}",
            if haptics.play_index(index) {
                "OK"
            } else {
                "FAIL"
            }
        );
        std::thread::sleep(Duration::from_millis(1500));
    }
    println!("Done.");
}

async fn run(haptics: Haptics, cfg: config::Config) -> Result<()> {
    haptics.connect();
    info!("mx4d started (device={})", haptics.device_path());

    let mut modules = tokio::task::JoinSet::new();
    #[cfg(feature = "haptic")]
    if cfg.haptic.enabled {
        modules.spawn(mx4_haptic::run(haptics.clone(), cfg.haptic.clone()));
    }
    let _ = &cfg;

    let mut term = signal(SignalKind::terminate())?;
    let mut int = signal(SignalKind::interrupt())?;
    let result = tokio::select! {
        _ = term.recv() => { info!("Received SIGTERM, shutting down"); Ok(()) }
        _ = int.recv() => { info!("Received SIGINT, shutting down"); Ok(()) }
        Some(done) = modules.join_next() => done?,
    };
    info!("Bye.");
    result
}
