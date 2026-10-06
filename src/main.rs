//! AION 2 damage meter for Linux.

mod capture;
mod db;
mod dispatcher;
mod engine;
mod names;
mod overlay;
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
}

#[derive(Clone, Copy, ValueEnum)]
enum Lang {
    De,
    En,
}

#[derive(Subcommand)]
enum Command {
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
    /// Show overlay and capture state without changing anything.
    Status,
}

fn status_text(live: &serde_json::Value) -> String {
    let yes_no = |v: &serde_json::Value| if v.as_bool() == Some(true) { "ja" } else { "nein" };
    let o = &live["overlay"];
    let c = &live["capture"];
    let connection = if c["permission"].as_bool() != Some(true) {
        "keine Capture-Berechtigung (setcap fehlt)".to_string()
    } else if let Some(port) = c["locked_port"].as_u64() {
        format!("verbunden (Port {port}, {})", c["device"].as_str().unwrap_or("?"))
    } else if c["game_running"].as_bool() == Some(true) {
        "Spiel läuft, suche Verbindung".to_string()
    } else {
        "AION2 nicht gestartet".to_string()
    };
    format!(
        "Overlay sichtbar: {}\nOverlay gesperrt: {}\nVerbindung:       {}\nCharakter:        {}\nOrt:              {}\nPing:             {}",
        yes_no(&o["visible"]),
        yes_no(&o["locked"]),
        connection,
        live["character"].as_str().unwrap_or("–"),
        live["dungeon"].as_str().unwrap_or("–"),
        live["ping_ms"].as_i64().map(|p| format!("{p} ms")).unwrap_or_else(|| "–".into()),
    )
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
    if cli.x11 {
        // winit picks Wayland whenever this is set. SAFETY: no other thread
        // exists yet.
        unsafe { std::env::remove_var("WAYLAND_DISPLAY") };
    }
    let addr = SocketAddr::new(cli.listen, cli.port);

    if let Some(Command::Ctl { action }) = cli.command {
        let local = SocketAddr::new(if cli.listen.is_unspecified() { [127, 0, 0, 1].into() } else { cli.listen }, cli.port);
        let (method, path) = match action {
            Action::ToggleLock => ("POST", "/api/overlay/toggle-lock"),
            Action::ToggleVisible => ("POST", "/api/overlay/toggle-visible"),
            Action::Reset => ("POST", "/api/reset"),
            Action::Status => ("GET", "/api/live"),
        };
        let body = web::request(local, method, path).context("Läuft aion2-meter?")?;
        if matches!(action, Action::Status) {
            let mut live: serde_json::Value = serde_json::from_str(&body)?;
            // The live snapshot trails a toggle by up to half a second.
            live["overlay"] = serde_json::from_str(&web::request(local, "GET", "/api/overlay")?)?;
            println!("{}", status_text(&live));
        } else {
            println!("{body}");
        }
        return Ok(());
    }

    let db_path = cli.db.unwrap_or_else(default_db);
    let database = db::Db::open(&db_path).with_context(|| format!("Datenbank {}", db_path.display()))?;
    tracing::info!("Database: {}", db_path.display());
    let lang = match cli.lang {
        Lang::De => "de",
        Lang::En => "en",
    };
    let engine = engine::Engine::new(database, lang);

    // Capture → parser
    let permission = capture::has_capture_permission();
    engine.set_permission(permission);
    if !permission {
        tracing::error!(
            "No permission to capture packets. Run once:  sudo setcap cap_net_raw=ep {}",
            std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| "aion2-meter".into())
        );
    }
    let (tx, rx) = mpsc::sync_channel(8192);
    {
        let engine = engine.clone();
        std::thread::Builder::new().name("capture".into()).spawn(move || {
            if let Err(e) = capture::run(tx) {
                tracing::error!("Capture stopped: {e}");
                engine.set_capture_error(Some(e.to_string()));
            }
        })?;
    }
    {
        let dispatcher = dispatcher::Dispatcher::new(engine.clone(), !cli.any_process);
        if cli.any_process {
            engine.set_game_running(true);
        }
        std::thread::Builder::new().name("parser".into()).spawn(move || dispatcher.run(rx))?;
    }
    {
        let engine = engine.clone();
        std::thread::Builder::new().name("meter".into()).spawn(move || engine.run_ticks())?;
    }

    // Web dashboard
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build()?;
    let web = runtime.spawn(web::serve(engine.clone(), addr));

    let shown = if addr.ip().is_unspecified() { SocketAddr::new([127, 0, 0, 1].into(), addr.port()) } else { addr };
    let url = format!("http://{shown}/");
    if cli.no_overlay {
        runtime.block_on(web)??;
        return Ok(());
    }
    overlay::Overlay::new(engine, url)
        .run()
        .map_err(|e| anyhow::anyhow!("Overlay: {e}"))?;
    Ok(())
}
