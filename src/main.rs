//! AION 2 damage meter for Linux.

mod analytics;
mod buffs;
mod capture;
mod community;
mod db;
mod dispatcher;
mod encounters;
mod engine;
mod instances;
mod names;
mod overlay;
mod reference_bundle;
mod replay;
mod skills;
mod tcp;
mod updates;
mod web;

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::mpsc;

use anyhow::Context;
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(name = "aion2-meter", version, about = "AION 2 Damage Meter für Linux")]
struct Cli {
    /// Port of the web dashboard.
    #[arg(long, default_value_t = 8787, global = true)]
    port: u16,
    /// Address the dashboard listens on. 0.0.0.0 makes it reachable from
    /// other devices in your network (phone, second PC).
    #[arg(long, default_value = "127.0.0.1", global = true)]
    listen: IpAddr,
    #[command(subcommand)]
    command: Option<Command>,
    /// SQLite database [default: ~/.local/share/aion2-meter/meter.db]
    #[arg(long)]
    db: Option<PathBuf>,
    /// Override the built-in offline community reference database with a local file.
    #[arg(long)]
    reference_db: Option<PathBuf>,
    /// Language of skill and monster names.
    #[arg(long, value_enum, default_value_t = Lang::De)]
    lang: Lang,
    /// Run without the overlay window (dashboard only).
    #[arg(long)]
    no_overlay: bool,
    /// Run the overlay through XWayland, where "always on top" works without a KWin rule.
    #[arg(long)]
    x11: bool,
    /// Do not wait for an AION2.exe process before capturing.
    #[arg(long)]
    any_process: bool,
    /// Record the game connection from the start (see `ctl record`).
    #[arg(long)]
    record: bool,
}

#[derive(Clone, Copy, ValueEnum)]
enum Lang {
    De,
    En,
}

#[derive(Subcommand)]
enum Command {
    /// Build an offline SQLite reference database from provider JSON snapshots.
    BuildReferences {
        #[arg(long)]
        output: PathBuf,
        #[arg(required = true)]
        snapshots: Vec<PathBuf>,
    },
    /// Decode an offline capture. No game, packet privileges or dashboard required.
    Replay {
        file: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Control a running meter (bind these to KDE shortcuts).
    Ctl {
        #[arg(value_enum)]
        action: Action,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum Action {
    /// Lock/unlock the overlay (click-through).
    ToggleLock,
    /// Show/hide the overlay.
    ToggleVisible,
    /// Reset the live meter.
    Reset,
    /// End the expedition run and start a new one (after a restart).
    NewRun,
    /// Start/stop recording the game connection to
    /// ~/.local/share/aion2-meter/captures (to decode more packets).
    Record,
    /// Show overlay and capture state without changing anything.
    Status,
}

fn status_text(live: &serde_json::Value) -> String {
    let yes_no = |v: &serde_json::Value| {
        if v.as_bool() == Some(true) {
            "ja"
        } else {
            "nein"
        }
    };
    let o = &live["overlay"];
    let c = &live["capture"];
    let connection = if c["permission"].as_bool() != Some(true) {
        "keine Capture-Berechtigung (setcap fehlt)".to_string()
    } else if let Some(port) = c["locked_port"].as_u64() {
        format!(
            "verbunden (Port {port}, {})",
            c["device"].as_str().unwrap_or("?")
        )
    } else if c["game_running"].as_bool() == Some(true) {
        "Spiel läuft, suche Verbindung".to_string()
    } else {
        "AION2 nicht gestartet".to_string()
    };
    format!(
        "Overlay sichtbar: {}\nOverlay gesperrt: {}\nVerbindung:       {}\nCharakter:        {}\nOrt:              {}\nPing:             {}\nMitschnitt:       {}",
        yes_no(&o["visible"]),
        yes_no(&o["locked"]),
        connection,
        live["character"].as_str().unwrap_or("–"),
        live["dungeon"].as_str().unwrap_or("–"),
        live["ping_ms"]
            .as_i64()
            .map(|p| format!("{p} ms"))
            .unwrap_or_else(|| "–".into()),
        recording_text(c),
    )
}

fn recording_text(capture: &serde_json::Value) -> String {
    if let Some(path) = capture["recording"].as_str() {
        format!("läuft → {path}")
    } else if capture["recording_requested"].as_bool() == Some(true) {
        "wartet auf die Spielverbindung".into()
    } else if let Some(e) = capture["recording_error"].as_str() {
        format!("Fehler: {e}")
    } else {
        "aus".into()
    }
}

fn default_db() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("aion2-meter").join("meter.db")
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,a2tools_dps_meter_lib=warn".into()),
        )
        .init();

    let cli = Cli::parse();
    if let Some(Command::BuildReferences {
        ref output,
        ref snapshots,
    }) = cli.command
    {
        let count = reference_bundle::build(output, snapshots)?;
        println!(
            "{count} Referenzdatensätze gespeichert: {}",
            output.display()
        );
        return Ok(());
    }
    if cli.x11 {
        // winit picks Wayland whenever this is set. SAFETY: no other thread
        // exists yet.
        unsafe { std::env::remove_var("WAYLAND_DISPLAY") };
    }
    let addr = SocketAddr::new(cli.listen, cli.port);

    if let Some(Command::Replay {
        ref file,
        ref output,
    }) = cli.command
    {
        let report = replay::run(file)?;
        let text = serde_json::to_string_pretty(&report)?;
        if let Some(path) = output {
            std::fs::write(path, text)?;
        } else {
            println!("{text}");
        }
        return Ok(());
    }
    if let Some(Command::Ctl { action }) = cli.command {
        let local = SocketAddr::new(
            if cli.listen.is_unspecified() {
                [127, 0, 0, 1].into()
            } else {
                cli.listen
            },
            cli.port,
        );
        let (method, path) = match action {
            Action::ToggleLock => ("POST", "/api/overlay/toggle-lock"),
            Action::ToggleVisible => ("POST", "/api/overlay/toggle-visible"),
            Action::Reset => ("POST", "/api/reset"),
            Action::NewRun => ("POST", "/api/run/new"),
            Action::Record => ("POST", "/api/record/toggle"),
            Action::Status => ("GET", "/api/live"),
        };
        let body = web::request(local, method, path).context("Läuft aion2-meter?")?;
        if matches!(action, Action::Status) {
            let mut live: serde_json::Value = serde_json::from_str(&body)?;
            // The live snapshot trails a toggle by up to half a second.
            live["overlay"] = serde_json::from_str(&web::request(local, "GET", "/api/overlay")?)?;
            println!("{}", status_text(&live));
        } else if matches!(action, Action::Record) {
            let r: serde_json::Value = serde_json::from_str(&body)?;
            let dir = r["dir"].as_str().unwrap_or_default();
            if r["recording"].as_bool() == Some(true) {
                println!("Mitschnitt läuft, sobald das Spiel verbunden ist. Ordner: {dir}");
            } else {
                println!("Mitschnitt gestoppt. Ordner: {dir}");
            }
        } else {
            println!("{body}");
        }
        return Ok(());
    }

    if !cli.no_overlay && (cli.x11 || std::env::var_os("WAYLAND_DISPLAY").is_none()) {
        overlay::check_x11_dependencies()?;
    }
    let db_path = cli.db.unwrap_or_else(default_db);
    let database =
        db::Db::open(&db_path).with_context(|| format!("Datenbank {}", db_path.display()))?;
    let bundle = if let Some(path) = cli.reference_db {
        tracing::info!("Offline reference database: {}", path.display());
        reference_bundle::read(&path)?
    } else {
        tracing::info!("Using built-in offline community reference database");
        reference_bundle::read_bundled()?
    };
    database.import_provider_archives(&bundle.archives)?;
    for snapshot in bundle.snapshots {
        database.import_community(snapshot)?;
    }
    tracing::info!("Database: {}", db_path.display());
    let lang = match cli.lang {
        Lang::De => "de",
        Lang::En => "en",
    };
    let captures = db_path
        .parent()
        .map(|p| p.join("captures"))
        .unwrap_or_else(|| PathBuf::from("captures"));
    let engine = engine::Engine::new(database, lang, captures);
    if cli.record {
        engine.set_recording(true);
    }

    // Fail before capture threads or the overlay start if another meter owns
    // the port. The asynchronous server task cannot report startup errors to
    // a running native UI.
    let listener = std::net::TcpListener::bind(addr)
        .with_context(|| format!("Dashboard-Adresse {addr} ist nicht verfügbar"))?;
    listener.set_nonblocking(true)?;
    let addr = listener.local_addr()?;

    // Capture → parser
    let permission = capture::has_capture_permission();
    engine.set_permission(permission);
    if !permission {
        tracing::error!(
            "No permission to capture packets. Run once:  sudo setcap cap_net_raw=ep {}",
            std::env::current_exe()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "aion2-meter".into())
        );
    }
    let (tx, rx) = mpsc::sync_channel(8192);
    let capture_thread = {
        let engine = engine.clone();
        std::thread::Builder::new()
            .name("capture".into())
            .spawn(move || {
                if let Err(e) = capture::run(tx, || engine.is_stopping()) {
                    tracing::error!("Capture stopped: {e}");
                    engine.set_capture_error(Some(e.to_string()));
                }
            })?
    };
    let parser_thread = {
        let dispatcher = dispatcher::Dispatcher::new(engine.clone(), !cli.any_process);
        if cli.any_process {
            engine.set_game_running(true);
        }
        std::thread::Builder::new()
            .name("parser".into())
            .spawn(move || dispatcher.run(rx))?
    };
    let meter_thread = {
        let engine = engine.clone();
        std::thread::Builder::new()
            .name("meter".into())
            .spawn(move || engine.run_ticks())?
    };

    // Web dashboard
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let mut web = runtime.spawn(web::serve(engine.clone(), listener));
    let signal_engine = engine.clone();
    let signals = runtime.spawn(async move {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! {
            result = tokio::signal::ctrl_c() => { result?; }
            _ = terminate.recv() => {}
        }
        if !signal_engine.shutdown() {
            anyhow::bail!("Kampf konnte beim Beenden nicht gespeichert werden");
        }
        Ok::<_, anyhow::Error>(())
    });

    let shown = if addr.ip().is_unspecified() {
        SocketAddr::new([127, 0, 0, 1].into(), addr.port())
    } else {
        addr
    };
    let url = format!("http://{shown}/");
    let result = if cli.no_overlay {
        runtime.block_on(async {
            tokio::select! {
                result = &mut web => result.map_err(anyhow::Error::from).and_then(|r| r),
                _ = async {
                    while !engine.is_stopping() {
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    }
                } => {
                    // A slow HTTP client must not prevent termination forever.
                    match tokio::time::timeout(std::time::Duration::from_secs(3), &mut web).await {
                        Ok(result) => result.map_err(anyhow::Error::from).and_then(|r| r),
                        Err(_) => { web.abort(); Ok(()) }
                    }
                }
            }
        })
    } else {
        overlay::Overlay::new(engine.clone(), url)
            .run()
            .map_err(|e| anyhow::anyhow!("Overlay: {e}"))
    };
    let saved = engine.shutdown();
    for (name, thread) in [
        ("capture", capture_thread),
        ("parser", parser_thread),
        ("meter", meter_thread),
    ] {
        if thread.join().is_err() {
            tracing::error!("Thread {name} stopped unexpectedly");
        }
    }
    if signals.is_finished() {
        runtime.block_on(signals)??;
    } else {
        signals.abort();
    }
    result?;
    anyhow::ensure!(saved, "Kampf konnte beim Beenden nicht gespeichert werden");
    Ok(())
}
