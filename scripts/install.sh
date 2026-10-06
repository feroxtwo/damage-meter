#!/usr/bin/env bash
# Builds and installs AION2 Meter for the current user (Fedora / KDE Plasma).
#   ./scripts/install.sh
set -euo pipefail
cd "$(dirname "$0")/.."

PREFIX="${PREFIX:-$HOME/.local}"
BIN="$PREFIX/bin/aion2-meter"

if ! command -v cargo >/dev/null; then
  echo "Rust fehlt. Installiere es mit:  sudo dnf install rust cargo gcc   (oder https://rustup.rs)"
  exit 1
fi

echo "==> Baue (Release) …"
cargo build --release

echo "==> Installiere nach $BIN"
install -Dm755 target/release/aion2-meter "$BIN"
install -Dm644 packaging/aion2-meter.desktop           "$PREFIX/share/applications/aion2-meter.desktop"
install -Dm644 packaging/aion2-meter-dashboard.desktop "$PREFIX/share/applications/aion2-meter-dashboard.desktop"
install -Dm644 packaging/aion2-meter.svg               "$PREFIX/share/icons/hicolor/scalable/apps/aion2-meter.svg"
sed -i "s|^Exec=aion2-meter|Exec=$BIN|" "$PREFIX/share/applications/aion2-meter.desktop"

echo "==> Erlaube Paketmitschnitt (einmalig sudo, nötig nach jedem Update)"
sudo setcap cap_net_raw=ep "$BIN"

if [[ "${XDG_CURRENT_DESKTOP:-}" == *KDE* ]] && command -v kwriteconfig6 >/dev/null; then
  echo "==> KWin-Regel: Overlay bleibt über dem Spiel"
  ./scripts/install-kwin-rule.sh
fi

update-desktop-database "$PREFIX/share/applications" 2>/dev/null || true
echo
echo "Fertig. Starte 'AION2 Meter' aus dem Startmenü oder mit:  aion2-meter"
echo "Dashboard: http://127.0.0.1:8787/"
