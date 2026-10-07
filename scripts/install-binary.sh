#!/usr/bin/env bash
# Install a downloaded archive or a source-built binary, including live upgrades.
set -euo pipefail
cd "$(dirname "$0")/.."
PREFIX=${PREFIX:-$HOME/.local}
binary=${METER_BINARY:-aion2-meter}
BIN="$PREFIX/bin/aion2-meter"
source scripts/desktop-exec.sh
exec_path=$(desktop_exec_quote "$BIN")
[[ -x "$binary" ]] || { echo "Binary fehlt: $binary" >&2; exit 1; }
mkdir -p "$PREFIX/bin" "$PREFIX/share/applications"
# Replace the inode only after preparation: running versions keep their old inode.
temporary=$(mktemp "$PREFIX/bin/.aion2-meter.XXXXXXXX")
trap 'rm -f "$temporary"' EXIT
install -m755 "$binary" "$temporary"
sudo setcap cap_net_raw=ep "$temporary"
mv -f "$temporary" "$BIN"
install -Dm644 packaging/aion2-meter.desktop "$PREFIX/share/applications/aion2-meter.desktop"
install -Dm644 packaging/aion2-meter-dashboard.desktop "$PREFIX/share/applications/aion2-meter-dashboard.desktop"
install -Dm644 packaging/aion2-meter.svg "$PREFIX/share/icons/hicolor/scalable/apps/aion2-meter.svg"
# Desktop Exec uses its own quoting rules, not shell quoting. Escape literal % too.
desktop="$PREFIX/share/applications/aion2-meter.desktop"
while IFS= read -r line; do
    if [[ "$line" == Exec=* ]]; then printf 'Exec=%s\n' "$exec_path"; else printf '%s\n' "$line"; fi
done < packaging/aion2-meter.desktop > "$desktop"
if [[ "${XDG_CURRENT_DESKTOP:-}" == *KDE* ]] && command -v kwriteconfig6 >/dev/null; then
    ./scripts/install-kwin-rule.sh
fi
update-desktop-database "$PREFIX/share/applications" 2>/dev/null || true
printf 'Installiert: %s\nLaufende Version beenden und neu starten. Einstellungen bleiben erhalten.\nDashboard: http://127.0.0.1:8787/\n' "$BIN"
