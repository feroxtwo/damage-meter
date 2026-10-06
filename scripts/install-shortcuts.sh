#!/usr/bin/env bash
# Registers global KDE shortcuts for the overlay, active immediately.
#
#   ./scripts/install-shortcuts.sh                         defaults below
#   LOCK=Ctrl+Ü VISIBLE=Ctrl+Shift+F10 ./scripts/install-shortcuts.sh
#   ./scripts/install-shortcuts.sh --remove
#
# Each shortcut is a command-shortcut .desktop file plus a registration with
# kglobalaccel over D-Bus. Writing kglobalshortcutsrc alone is only picked up
# after KWin restarts.
set -euo pipefail

BIN="${BIN:-$HOME/.local/bin/aion2-meter}"
APPS="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
LOCK="${LOCK:-Ctrl+Shift+F9}"
VISIBLE="${VISIBLE:-Ctrl+Shift+F10}"
RESET="${RESET:-Ctrl+Shift+F11}"

# "Ctrl+Shift+F9" -> Qt key code (modifier bits + key).
qt_key() {
  local spec="$1" code=0 part key
  IFS='+' read -ra parts <<<"$spec"
  for part in "${parts[@]}"; do
    case "${part,,}" in
      ctrl|strg|control) code=$((code | 0x04000000)) ;;
      shift|umschalt)    code=$((code | 0x02000000)) ;;
      alt)               code=$((code | 0x08000000)) ;;
      meta|super|win)    code=$((code | 0x10000000)) ;;
      *) key="$part" ;;
    esac
  done
  case "$key" in
    [Ff][1-9]|[Ff]1[0-2]) code=$((code | (0x01000030 + ${key:1} - 1))) ;;
    [A-Za-z0-9])          code=$((code | $(printf '%d' "'${key^^}"))) ;;
    Ü|ü) code=$((code | 0xDC)) ;;
    Ö|ö) code=$((code | 0xD6)) ;;
    Ä|ä) code=$((code | 0xC4)) ;;
    *) echo "Unbekannte Taste in '$spec'" >&2; return 1 ;;
  esac
  echo "$code"
}

# component id, friendly name, ctl action, key spec
register() {
  local id="net.local.aion2-meter-$1.desktop" name="$2" action="$3" spec="$4"
  local file="$APPS/$id"
  mkdir -p "$APPS"
  cat >"$file" <<EOF
[Desktop Entry]
Type=Application
Name=$name
Exec=$BIN ctl $action
Icon=aion2-meter
NoDisplay=true
StartupNotify=false
X-KDE-GlobalAccel-CommandShortcut=true
EOF
  local code
  code="$(qt_key "$spec")"
  busctl --user call org.kde.kglobalaccel /kglobalaccel org.kde.KGlobalAccel \
    doRegister as 4 "$id" _launch "$name" "$name"
  # Flags 6 = SetPresent | NoAutoloading: use these keys now, ignore saved ones.
  busctl --user call org.kde.kglobalaccel /kglobalaccel org.kde.KGlobalAccel \
    setShortcutKeys 'asa(ai)u' 4 "$id" _launch "$name" "$name" 1 1 "$code" 6 >/dev/null
  printf '  %-16s %s\n' "$spec" "$name"
}

unregister() {
  local id="net.local.aion2-meter-$1.desktop"
  busctl --user call org.kde.kglobalaccel /kglobalaccel org.kde.KGlobalAccel \
    unregister ss "$id" _launch >/dev/null 2>&1 || true
  rm -f "$APPS/$id"
}

if ! command -v busctl >/dev/null; then
  echo "busctl fehlt (systemd)." >&2
  exit 1
fi

if [[ "${1:-}" == "--remove" ]]; then
  for a in lock visible reset; do unregister "$a"; done
  echo "Tastenkürzel entfernt."
  exit 0
fi

[ -x "$BIN" ] || echo "Hinweis: $BIN existiert noch nicht. Erst ./scripts/install.sh ausführen." >&2
echo "Tastenkürzel (sofort aktiv):"
register lock    "AION2 Meter: Overlay sperren/entsperren" toggle-lock    "$LOCK"
register visible "AION2 Meter: Overlay ein-/ausblenden"    toggle-visible "$VISIBLE"
register reset   "AION2 Meter: Meter zurücksetzen"         reset          "$RESET"
update-desktop-database "$APPS" 2>/dev/null || true
echo "Ändern: Systemeinstellungen → Tastatur → Kurzbefehle → „AION2 Meter“."
