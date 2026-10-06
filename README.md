# AION2 Meter

Eigenständiger Damage Meter für **AION 2 unter Linux** (getestet als Ziel: Fedora mit KDE Plasma, Wayland).
Er liest den Netzwerkverkehr des Spiels mit, zeigt ein **Overlay im Spiel** und speichert jede
**Expedition** mit Gruppe und Bosskämpfen in einer **SQLite-Datenbank**, die du im Browser auswerten kannst.

| | |
|---|---|
| **Overlay** | Kleines, transparentes Fenster über dem Spiel: Ziel mit HP-Balken, Kampfzeit, Gruppen-DPS, pro Spieler Schaden, DPS und Anteil (Klassenfarben, du bist gold umrandet), Ping und aktueller Dungeon. |
| **Dashboard** | `http://127.0.0.1:8787/` im Browser: Live-Meter, alle Runs mit Gruppe und Bossen, Skill-Aufschlüsselung pro Kampf (Krit/Rücken/Perfekt), Statistiken. |
| **Statistik** | **Top 5 Mitspieler**, mit denen du am häufigsten in Expeditionen warst, Runs pro Dungeon mit Bestzeit, deine beste DPS pro Boss, Aktivität der letzten 30 Tage. |
| **Datenbank** | `~/.local/share/aion2-meter/meter.db` (SQLite). Ein Run beginnt beim Betreten einer Instanz und endet beim Verlassen. |

Kein Discord, kein Account, nichts verlässt deinen Rechner.

## Installation (Fedora / KDE)

```bash
sudo dnf install rust cargo gcc git
git clone https://github.com/feroxtwo/aion2-meter.git
cd aion2-meter
./scripts/install.sh
```

Das Skript

1. baut das Programm (`cargo build --release`, beim ersten Mal einige Minuten),
2. installiert es nach `~/.local/bin/aion2-meter` mit Startmenü-Einträgen „AION2 Meter“ und „AION2 Meter Dashboard“,
3. erlaubt ihm per `sudo setcap cap_net_raw=ep` den Paketmitschnitt (der Meter selbst läuft **nicht** als root),
4. legt unter KDE eine **KWin-Fensterregel** an, damit das Overlay über dem Spiel bleibt.

Nach einem Update (`git pull && ./scripts/install.sh`) wird `setcap` erneut ausgeführt, weil eine neu gebaute Datei die Berechtigung verliert.

## Benutzung

1. AION 2 über Steam/Proton starten, am besten **randloses Fenster** oder **Fenstermodus**.
2. „AION2 Meter“ starten. Unten im Overlay zeigt ein Punkt den Zustand:
   grau = Spiel nicht gefunden, gelb = suche Verbindung, grün = verbunden.
3. Kämpfen. Bosskämpfe werden automatisch gespeichert, Runs beim Verlassen der Instanz abgeschlossen.

**Overlay bedienen:** Kopfzeile ziehen zum Verschieben, Rechtsklick für Menü (Zurücksetzen, Ziel-Modus,
Sperren, Dashboard, Beenden). **Gesperrt** gehen alle Klicks durch ans Spiel. Entsperren geht über das
Dashboard (Tab „Overlay“) oder ein Tastenkürzel.

**Tastenkürzel:** Wayland erlaubt Programmen keine globalen Hotkeys. Leg sie in
*Systemeinstellungen → Tastatur → Kurzbefehle → Neu hinzufügen → Befehl oder Skript* an:

| Befehl | Wirkung |
|---|---|
| `aion2-meter ctl toggle-lock` | Overlay sperren / entsperren |
| `aion2-meter ctl toggle-visible` | Overlay ein- / ausblenden |
| `aion2-meter ctl reset` | Live-Meter zurücksetzen |

### Optionen

```
aion2-meter [--no-overlay] [--x11] [--port 8787] [--listen 127.0.0.1] [--lang de|en] [--db PFAD] [--any-process]
```

- `--no-overlay` nur Mitschnitt + Dashboard (z. B. Dashboard auf zweitem Monitor).
- `--x11` Overlay über XWayland, dort klappt „immer im Vordergrund“ auch ohne KWin-Regel.
- `--listen 0.0.0.0` Dashboard im Heimnetz erreichbar (Handy, Zweit-PC). Ohne Passwort, also nur im eigenen Netz.
- `--lang en` Skill- und Monsternamen auf Englisch.
- `--any-process` nicht auf einen `AION2.exe`-Prozess warten (z. B. Spiel in VM/Container).
- `/overlay` im Dashboard ist eine Browser-Variante des Overlays, z. B. als OBS-Browserquelle.

## Fehlerbehebung

| Problem | Lösung |
|---|---|
| Overlay rot: „keine Capture-Berechtigung“ | `sudo setcap cap_net_raw=ep ~/.local/bin/aion2-meter` |
| Overlay bleibt grau, obwohl das Spiel läuft | Starte mit `--any-process`. Bleibt es dann gelb, sieht der Meter den Spielverkehr nicht (VPN/Ping-Reducer?). Log mit `RUST_LOG=debug aion2-meter` |
| Overlay verschwindet hinter dem Spiel | Spiel randlos/Fenster statt Vollbild. `./scripts/install-kwin-rule.sh` erneut ausführen, oder in *Systemeinstellungen → Fensterverwaltung → Fensterregeln* für `aion2-meter` „Ebene: Overlay“ erzwingen. Alternativ `aion2-meter --x11`. |
| Overlay hat schwarzen statt transparenten Hintergrund | `aion2-meter --x11` probieren und melden. |
| Nach einem Spiel-Patch kein Schaden mehr | Der Parser stammt aus A2Tools (siehe unten). `rev` in `Cargo.toml` auf den neuesten Commit von A2Tools setzen und neu installieren. |

## Wie es funktioniert

```
Netzwerk ──AF_PACKET──▶ capture.rs ──▶ dispatcher.rs ──▶ A2Tools-Parser ──▶ engine.rs ──┬──▶ Overlay (egui)
 (alle Interfaces,        TCP-Payload    erkennt die       (Schaden, Namen,    Live-Werte,  ├──▶ Dashboard (axum, :8787)
  inkl. VPN/tun)                          Spielverbindung   Party, Instanz)     Run-Tracking └──▶ SQLite
```

- **Mitschnitt** direkt über einen `AF_PACKET`-Socket, ohne libpcap. Braucht nur `CAP_NET_RAW`.
- **Protokoll-Parser und Kampfauswertung** kommen aus [A2Tools DPS Meter](https://github.com/taengu/A2Tools-DPS-Meter)
  (GPL-3.0) als Bibliothek, ohne dessen Tauri-Oberfläche. So profitiert dieser Meter direkt von deren
  Anpassungen an Spiel-Patches. Die Datendateien in `data/` (Skill-, NPC- und Dungeon-Namen) stammen ebenfalls von dort.
- **Runs:** Die Instanz-ID kommt aus dem Party-Roster des Spiels. Wechselt sie, endet der alte Run und ein neuer beginnt.
  Die Gruppe eines Runs sind alle Spieler aus dem Roster plus alle, die in einem Bosskampf mit dir Schaden gemacht haben.
  Runs ohne Bosskampf und ohne Mitspieler werden verworfen.

### Datenbank

| Tabelle | Inhalt |
|---|---|
| `runs` | Dungeon, Schwierigkeit, Start, Ende, dein Charakter, Notiz |
| `run_members` | Gruppe pro Run (Klasse, Server, Level, Kampfkraft) |
| `fights` | Bosskämpfe mit Dauer, Gesamtschaden und vollständigem Kampf-Datensatz (JSON) |
| `fight_players` | Schaden, DPS, Anteil, Heilung pro Spieler und Kampf |
| `my_characters` | deine Charaktere (werden aus den Mitspieler-Statistiken ausgenommen) |

Die Datei kann jederzeit mit `sqlite3` oder DB Browser for SQLite geöffnet werden.

## Entwicklung

```bash
cargo test
cargo run -- --no-overlay --any-process   # ohne setcap: Dashboard läuft, Mitschnitt meldet fehlende Berechtigung
```

## Lizenz

GPL-3.0-or-later, wie A2Tools DPS Meter, dessen Parser und Daten dieses Projekt nutzt.
Die Nutzung von Drittanbieter-Tools kann gegen die Nutzungsbedingungen des Spiels verstoßen; Verwendung auf eigenes Risiko.
