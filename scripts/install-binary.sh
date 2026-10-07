#!/usr/bin/env bash
# Install a downloaded release archive without Rust or Node.js.
set -euo pipefail
cd "$(dirname "$0")/.."
PREFIX=${PREFIX:-$HOME/.local}
BIN="$PREFIX/bin/aion2-meter"
install -Dm755 aion2-meter "$BIN"
install -Dm644 packaging/aion2-meter.desktop "$PREFIX/share/applications/aion2-meter.desktop"
install -Dm644 packaging/aion2-meter-dashboard.desktop "$PREFIX/share/applications/aion2-meter-dashboard.desktop"
install -Dm644 packaging/aion2-meter.svg "$PREFIX/share/icons/hicolor/scalable/apps/aion2-meter.svg"
sed -i "s|^Exec=aion2-meter|Exec=$BIN|" "$PREFIX/share/applications/aion2-meter.desktop"
sudo setcap cap_net_raw=ep "$BIN"
if [[ "${XDG_CURRENT_DESKTOP:-}" == *KDE* ]] && command -v kwriteconfig6 >/dev/null; then
    ./scripts/install-kwin-rule.sh
fi
update-desktop-database "$PREFIX/share/applications" 2>/dev/null || true
printf 'Installiert: %s\nDashboard: http://127.0.0.1:8787/\n' "$BIN"
