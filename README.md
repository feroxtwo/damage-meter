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

**Fedora Workstation / KDE Spin:**

```bash
sudo dnf install rust cargo gcc git
git clone https://github.com/feroxtwo/damage-meter.git
cd damage-meter
./scripts/install.sh
```

**Bazzite, Kinoite, Silverblue (rpm-ostree):** `dnf` funktioniert dort nicht. Rust kommt ohne root über rustup,
der Rest ist schon an Bord:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
git clone https://github.com/feroxtwo/damage-meter.git
cd damage-meter
./scripts/install.sh
```

Das Skript lädt `~/.cargo/env` selbst, ein neues Terminal nach rustup ist also nicht nötig.

> Solange das Repository **privat** ist, braucht `git clone` eine Anmeldung: entweder `gh auth login` und dann
> `gh repo clone feroxtwo/damage-meter`, oder auf GitHub *Code → Download ZIP* und das Archiv entpacken.

Das Skript

1. baut das Programm (`cargo build --release`, beim ersten Mal einige Minuten),
2. installiert es nach `~/.local/bin/aion2-meter` mit Startmenü-Einträgen „AION2 Meter“ und „AION2 Meter Dashboard“,
3. erlaubt ihm per `sudo setcap cap_net_raw=ep` den Paketmitschnitt (der Meter selbst läuft **nicht** als root),
4. legt unter KDE eine **KWin-Fensterregel** an, damit das Overlay über dem Spiel bleibt,
5. fragt, ob es die **Tastenkürzel** einrichten soll (siehe unten).

Nach einem Update (`git pull && ./scripts/install.sh`) wird `setcap` erneut ausgeführt, weil eine neu gebaute Datei die Berechtigung verliert.

## Benutzung

1. AION 2 über Steam/Proton starten, am besten **randloses Fenster** oder **Fenstermodus**.
2. „AION2 Meter“ starten. Unten im Overlay zeigt ein Punkt den Zustand:
   grau = Spiel nicht gefunden, gelb = suche Verbindung, grün = verbunden.
3. Kämpfen. Bosskämpfe werden automatisch gespeichert, Runs beim Verlassen der Instanz abgeschlossen.

**Overlay bedienen:** Kopfzeile ziehen zum Verschieben, Rechtsklick für Menü (Zurücksetzen, Ziel-Modus,
Sperren, Dashboard, Beenden). Klappt das Ziehen nicht, geht es unter KDE immer mit **Meta (Windows-Taste) +
Linksziehen**. **Gesperrt** gehen alle Klicks durch ans Spiel. Entsperren geht über das Dashboard (Tab „Overlay“)
oder ein Tastenkürzel.

**Tastenkürzel:** Wayland erlaubt Programmen keine eigenen globalen Hotkeys, deshalb registriert
`./scripts/install-shortcuts.sh` sie bei KDE (sofort aktiv, kein Neustart):

| Standard | Befehl | Wirkung |
|---|---|---|
| Strg+Umschalt+F9 | `~/.local/bin/aion2-meter ctl toggle-lock` | Overlay sperren / entsperren |
| Strg+Umschalt+F10 | `~/.local/bin/aion2-meter ctl toggle-visible` | Overlay ein- / ausblenden |
| Strg+Umschalt+F11 | `~/.local/bin/aion2-meter ctl reset` | Live-Meter zurücksetzen |

Andere Tasten: `LOCK=Strg+Ü VISIBLE=Strg+Ö RESET=Strg+Ä ./scripts/install-shortcuts.sh`. Entfernen:
`./scripts/install-shortcuts.sh --remove`. Danach lassen sie sich auch unter *Systemeinstellungen → Tastatur →
Kurzbefehle* ändern.

`~/.local/bin/aion2-meter ctl status` zeigt, ob das Overlay sichtbar und gesperrt ist und ob der Meter mit dem
Spiel verbunden ist, ohne etwas umzuschalten.

**Mehrere Charaktere:** Jeder Run merkt sich, mit welchem Charakter du gespielt hast. Oben im Dashboard
filtert eine Auswahl Runs und Statistik (Top-Mitspieler, DPS-Verlauf, Bestwerte) auf einen Charakter oder
zeigt alle zusammen. Deine eigenen Charaktere zählen nie als Mitspieler.

### Buffs, Debuffs und Mitschnitt

Bei jedem gespeicherten Bosskampf (Runs → Kampf → Skills) steht, wie lange jeder Spieler welche Buffs hatte
und welche Debuffs wie lange auf dem Boss lagen, jeweils in Prozent der Kampfzeit. Das Buff-Paket ist aus den
offenen Metern NOIA2 und AIon2-Dps-Meter übernommen.

Ausweichen und nDPS entschlüsselt bisher kein offener Meter. Damit sich das ändern kann, schneidet
`~/.local/bin/aion2-meter ctl record` (oder „Pakete mitschneiden“ im Dashboard bzw. im Overlay-Menü) die
Spielverbindung mit, bis du es nochmal aufrufst. Die Dateien (`*.a2mcap`, rohe TCP-Daten der Spielverbindung
mit Zeitstempel) landen in `~/.local/share/aion2-meter/captures/`. Sie enthalten alles, was der Server
deinem Client schickt, also auch Chat und Namen: nur weitergeben, wem du das zeigen willst.

### Optionen

```
aion2-meter [--no-overlay] [--x11] [--port 8787] [--listen 127.0.0.1] [--lang de|en] [--db PFAD] [--any-process] [--record]
```

- `--no-overlay` nur Mitschnitt + Dashboard (z. B. Dashboard auf zweitem Monitor).
- `--x11` Overlay über XWayland, dort klappt „immer im Vordergrund“ auch ohne KWin-Regel.
- `--listen 0.0.0.0` Dashboard im Heimnetz erreichbar (Handy, Zweit-PC). Ohne Passwort, also nur im eigenen Netz.
- `--lang en` Skill- und Monsternamen auf Englisch.
- `--any-process` nicht auf einen `AION2.exe`-Prozess warten (z. B. Spiel in VM/Container).
- `--record` gleich beim Start mitschneiden (siehe oben).
- `/overlay` im Dashboard ist eine Browser-Variante des Overlays, z. B. als OBS-Browserquelle.

## Fehlerbehebung

| Problem | Lösung |
|---|---|
| Overlay rot: „keine Capture-Berechtigung“ | `sudo setcap cap_net_raw=ep ~/.local/bin/aion2-meter` |
| Overlay bleibt grau, obwohl das Spiel läuft | Starte mit `--any-process`. Bleibt es dann gelb, sieht der Meter den Spielverkehr nicht (VPN/Ping-Reducer?). Log mit `RUST_LOG=debug aion2-meter` |
| Overlay verschwindet hinter dem Spiel | Spiel randlos/Fenster statt Vollbild. `./scripts/install-kwin-rule.sh` erneut ausführen, oder in *Systemeinstellungen → Fensterverwaltung → Fensterregeln* für `aion2-meter` „Ebene: Overlay“ erzwingen. Alternativ `aion2-meter --x11`. |
| Overlay lässt sich an der Kopfzeile nicht ziehen | Meta + Linksziehen. Wenn du die KWin-Regel vor Version 0.1.1 installiert hast: `./scripts/install-kwin-rule.sh` erneut ausführen (die alte Regel verbot dem Overlay den Fokus). |
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

## Modernisiertes Dashboard und technische Prüfung

Die Live-Ansicht zeigt Gruppen-DPS, eigene DPS, Gesamtschaden und Kampfdauer direkt als Kennzahlen. Runs und Statistik sind auch auf kleinen Bildschirmen bedienbar. Das Browser-Overlay unter `/overlay` berücksichtigt jetzt die gleichen Anzeige- und Streaming-Einstellungen wie das native Overlay.

![Dashboard mit Testdaten](docs/images/dashboard-desktop.png)

Befunde, Änderungen, Prüfungen und verbleibende Grenzen stehen in der [technischen Prüfung](docs/TECHNICAL_REVIEW.md).

### Entwicklung prüfen

```bash
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
python3 scripts/test-headless.py
npm ci
npx playwright install chromium
npm test
```

Node.js (ab Version 20) und Playwright werden ausschließlich für die Browserprüfungen benötigt. Die Anwendung selbst bleibt eine Rust-Binary mit eingebettetem HTML. `Cargo.lock` und `package-lock.json` gehören zum Repo.

Der HTTP-Server akzeptiert `localhost`, Loopback-IP-Adressen und die gebundene IP mit dem richtigen Port. Bei `--listen 0.0.0.0` sind IP-Adressen im Netzwerk erlaubt. Beliebige Domainnamen werden zum Schutz gegen DNS-Rebinding abgewiesen.
