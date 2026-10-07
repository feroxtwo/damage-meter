#!/usr/bin/env bash
# Builds and installs AION2 Meter for the current user (Fedora / KDE Plasma).
#   ./scripts/install.sh
set -euo pipefail
cd "$(dirname "$0")/.."

PREFIX="${PREFIX:-$HOME/.local}"
BIN="$PREFIX/bin/aion2-meter"

# A fresh rustup install only puts cargo on PATH in new terminals.
# shellcheck disable=SC1091
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"

if ! command -v cargo >/dev/null; then
  echo "Rust fehlt."
  if [ -e /run/ostree-booted ]; then
    # Bazzite, Kinoite, Silverblue: the system is read-only, dnf does not work.
    echo "Auf diesem System (rpm-ostree) installierst du Rust ohne root mit:"
    echo "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
  else
    echo "Installiere es mit:  sudo dnf install rust cargo gcc"
    echo "oder ohne root:      curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
  fi
  echo "Danach dieses Skript erneut starten."
  exit 1
fi

# The bundled SQLite and ring are C code: cargo needs a C compiler and linker.
if ! command -v cc >/dev/null && ! command -v gcc >/dev/null; then
  echo "C-Compiler (cc/gcc) fehlt. Er wird zum Bauen gebraucht (eingebaute SQLite)."
  if [ -e /run/ostree-booted ]; then
    echo "Auf Bazzite, Kinoite, Silverblue entweder das fertige Paket bzw. Linux-Archiv nutzen"
    echo "oder in einer Toolbox bauen und auf dem Host installieren:"
    echo "  toolbox create && toolbox run sudo dnf install -y gcc && toolbox run cargo build --release --locked"
    echo "  METER_BINARY=target/release/aion2-meter ./scripts/install-binary.sh   # außerhalb der Toolbox"
  else
    echo "Installiere ihn mit:  sudo dnf install gcc"
  fi
  exit 1
fi

echo "==> Baue (Release) …"
cargo build --release --locked

echo "==> Installiere nach $BIN"
METER_BINARY="${CARGO_TARGET_DIR:-target}/release/aion2-meter" ./scripts/install-binary.sh

if [[ "${XDG_CURRENT_DESKTOP:-}" == *KDE* ]] && [ -t 0 ]; then
  read -r -p "Tastenkürzel einrichten (Strg+Umschalt+F9/F10/F11)? [J/n] " answer
  if [[ ! "$answer" =~ ^[nN] ]]; then
    BIN="$BIN" ./scripts/install-shortcuts.sh
  fi
fi

echo
echo "Fertig. Starte 'AION2 Meter' aus dem Startmenü oder mit:  $BIN"
echo "Dashboard: http://127.0.0.1:8787/"
