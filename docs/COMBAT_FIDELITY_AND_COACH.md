# Combat Fidelity und persönlicher Performance Coach

## Implementiert in PR #25 (basierend auf main 65cf4e4)

- Upstream-Parser von A2Tools 2.0.52 (`d3cf6f9`) auf 2.0.54 (`82e53c1`) aktualisiert. Die neue Revision enthält u.a. die bessere Entscheidung, **nur eigene oder Gruppen-Bosse** zu verfolgen, Korrekturen der Namens-/Akteurszuordnung, DoT-/Summon-Behandlung und Packet-Framing. Änderungen an Parsermesswerten sind nur durch Replay- und Ingame-Gegenprüfung sicher beurteilbar; CI ersetzt diese realen Tests nicht.
- Selektiertes Ziel nutzt dessen **eigene** `last_damage_time` für den Kampfstatus. Andere Ziele dürfen einen ruhenden Boss nicht künstlich aktiv erscheinen lassen.
- Nach mindestens 3.000 ms ohne beobachteten Treffer ist der Zustand `paused`. Browser, OBS und natives Overlay zeigen dann **0 aktuelle Damage-DPS**, behalten jedoch Gesamtschaden, Anteile, erfasste historische Kampf-DPS und Kampfberichte. Heilungs-/eingehende Schadensmetriken bleiben eigenständig. Bei unbekannter Trefferzeit entsteht kein erfundener Pausenstatus.
- Ein Boss mit langen Mechanikphasen wird **nicht** automatisch abgeschlossen. Die bestehende Auto-Reset-Option bleibt konfigurierbar (Standard aus); das abschließende globale Reset-Sicherheitsgatter berücksichtigt alle Ziele, damit parallele Kämpfe nicht gelöscht werden. Vor jedem tatsächlichen Reset wird gespeichert.
- Der Performance Coach ist eine **beobachtende** Auswertung, keine kausale Rotationsempfehlung. Er vergleicht nur denselben Charakter, dieselbe Klasse, dieselbe Boss-NPC-ID und Schwierigkeit. Gemessene Schadensaktivität = Anteil gültiger 500-ms-Intervalle mit positivem Delta; keine Cast-Zahl, kein effektiver DPS durch externe Buffs. Datenlücken, unbestätigte Kills und unvollständige Timeseries werden sichtbar gekennzeichnet.
- Community-V2-Quellen benötigen `fight_dps`, `confirmed_kill`, `median_unique_players` und `patch_id`. V1-Bestandsdaten bleiben kompatibel, sind aber **nur Richtwerte**. Ein V2-Score erscheint erst bei passendem lokalen bestätigtem Kill ohne `analytics.partial`. Keine automatische Drittanbieterabfrage und kein Upload.

## In der CI zu prüfen

1. `cargo fmt --all --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked`, `cargo build --release --locked`, Rust 1.88 MSRV.
2. Browser-Tests mit quiet/active Status, erhaltenem Totalschaden und vorherigem Kampf; Node-Tests des Performance Coach mit Lücken und unterschiedlichen NPC-IDs.
3. Migration bestehender SQLite-Daten und V1-Importe; V2-Kohorten nach Boss, Schwierigkeit, Region, Kampf-KP, Zeitfenster, Killstatus; Schutz der Import-Löschoperation.
4. Replay-Messwerte (Skill-Schaden, Gesamt-DPS, Actor-/Summon-Zuordnung) zwischen alter und neuer Parserrevision anhand **identischer echter Aufzeichnungen** vergleichen. CI-Fakes und Beispiele sind kein Ersatz für Aufnahmen aus dem aktuellen EU-Client.

## Noch offen

- Echte Ingame-Abnahme auf Fedora/Wayland/Proton mit längerer Bossmechanikphase, mehreren gleichzeitigen Zielen, fremdem Feldboss und Disconnect.
- Vor Version 1.0 geklärte Lizenz/API für tatsächliche Community-Daten statt manuellem rechtmäßigem JSON-Import.
- Keine ungeprüften roten/grünen DPS-Veränderungen nach Parserupgrade behaupten. Ein importiertes Patchlabel ist eine Provider-Angabe, nicht kryptografisch bestätigt.
