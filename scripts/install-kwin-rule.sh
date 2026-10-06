#!/usr/bin/env bash
# Adds a KWin window rule that keeps the AION2 Meter overlay above the game on
# KDE Plasma (Wayland and X11): always on top, overlay layer, no border, not in
# the taskbar or Alt+Tab. Safe to run again; it replaces its own rule.
set -euo pipefail

RULE_ID="aion2-meter-overlay"
RC="kwinrulesrc"
read_cfg()  { kreadconfig6  --file "$RC" "$@" 2>/dev/null || kreadconfig5  --file "$RC" "$@"; }
write_cfg() { kwriteconfig6 --file "$RC" "$@" 2>/dev/null || kwriteconfig5 --file "$RC" "$@"; }

rules="$(read_cfg --group General --key rules || true)"
if [[ ",${rules}," != *",${RULE_ID},"* ]]; then
  rules="${rules:+${rules},}${RULE_ID}"
fi
count=$(awk -F, '{print NF}' <<<"$rules")

write_cfg --group General --key rules "$rules"
write_cfg --group General --key count "$count"

g=(--group "$RULE_ID")
write_cfg "${g[@]}" --key Description   "AION2 Meter Overlay"
write_cfg "${g[@]}" --key wmclass       "aion2-meter"
write_cfg "${g[@]}" --key wmclassmatch  1
write_cfg "${g[@]}" --key wmclasscomplete false
write_cfg "${g[@]}" --key types         1          # normal windows
write_cfg "${g[@]}" --key above         true
write_cfg "${g[@]}" --key aboverule     2          # force
write_cfg "${g[@]}" --key layer         overlay    # above borderless/fullscreen games (Plasma 6)
write_cfg "${g[@]}" --key layerrule     2
write_cfg "${g[@]}" --key noborder      true
write_cfg "${g[@]}" --key noborderrule  2
write_cfg "${g[@]}" --key skiptaskbar   true
write_cfg "${g[@]}" --key skiptaskbarrule 2
write_cfg "${g[@]}" --key skippager     true
write_cfg "${g[@]}" --key skippagerrule 2
write_cfg "${g[@]}" --key skipswitcher  true
write_cfg "${g[@]}" --key skipswitcherrule 2
write_cfg "${g[@]}" --key acceptfocus   false
write_cfg "${g[@]}" --key acceptfocusrule 2

# Tell KWin to reload its rules.
if command -v qdbus6 >/dev/null; then qdbus6 org.kde.KWin /KWin reconfigure
elif command -v qdbus >/dev/null; then qdbus org.kde.KWin /KWin reconfigure
else dbus-send --session --type=method_call --dest=org.kde.KWin /KWin org.kde.KWin.reconfigure
fi
echo "KWin-Regel '${RULE_ID}' installiert."
