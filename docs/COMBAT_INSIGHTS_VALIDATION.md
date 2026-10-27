# Combat insights: Softwareabnahme vom 22.10.2026

Die acht Verbesserungsbereiche sind implementiert. Diese Abnahme beschreibt Softwareprüfungen, keine bestätigte AION-2-Ingame-Messgenauigkeit.

## Erfolgreich

- Rust 1.88.0, `cargo test --locked --workspace`: **137 Anwendungstests und 143 Parser-Kern-Tests bestanden**. Ein bereits vorhandener Parser-Test bleibt ignoriert. Enthalten sind 4 Milliarden Schaden über Skilldetails, Live-Gesamtsumme und SQLite, getrennte Cast-/Trefferzahlen, Buff-Instanzen mit frühem Ende, atomare Abweisung beschädigter Endlisten, HP-Plausibilität, tatsächliche Todesframes, Reset-Grenzen, Leerlauf ohne Fehlalarm, Cooldown-Varianten/Reset sowie monotone Live- und deterministische Replay-Zeit.
- `cargo fmt --all --check` und `cargo clippy --locked --all-targets -- -D warnings`: bestanden.
- `cargo build --release --locked` mit Rust 1.88.0 und unverändert aktivem ThinLTO: erfolgreich. Der Compiler ist in `rust-toolchain.toml`, CI und Release-Workflow gepinnt. Rust 1.99.1 zeigte hier den bereits auf main dokumentierten Linkerfehler.
- Neue JS-Regressionsprüfungen sowie die vorhandenen ausführbaren Logiktests für Insights, Skills, Charts, Run-Auswertung, Community-UI und Coach bestanden.
- DOM-Prüfung des tatsächlichen `combat-insights.js`: gemeinsame Zeitachse, Replay-Regler und Interpolation, Namen-Escaping, Diagnose, Kontrast, alte Kämpfe sowie Suche, Auswahl, Entfernen und Speichern von Cooldown-Skills bestanden.
- Test mit echtem Release-Binary und HTTP-Server: OBS ist zunächst aus, isolierte GET-Endpunkte, begrenzter Snapshot, keine Dashboard-/Historienfreigabe, Privacy-Header, Schlüsselrotation und Deaktivierung bestanden. Unit-Tests prüfen außerdem zufällige Schlüssel und die Regeln für lokale Gegenstellen.
- Release-Headless-/CLI-Prüfung, Validierungsprüfungen und simulierter Installer-Test erfolgreich. Die Installerprüfung erteilt keine echten Capture-Rechte.
- Linux-TAR und Debian-Paket gebaut und geprüft: Binary, Version, Metadaten, Installationsskripte, Dokumentation und Prüfsummen. MIT-Hinweis und Parser-Provenienz werden mit ausgeliefert.

## Offen bzw. nicht ausführbar

- Die bestehende Playwright-Browsersuite konnte in dieser Umgebung nicht starten. Chromium bricht wegen verweigerter Unix-Sockets im Process-Singleton ab. Die DOM- und Logikprüfungen sind **kein Ersatz für eine visuelle Browserabnahme**. Der Fixture-Server der Suite liefert das neue Skript aus; die Suite bleibt in CI aktiv.
- Kein nativer Screenshot-/KDE-/Wayland-Test in dieser Umgebung. Darstellung, Skalierung, Transparenz und Click-through müssen auf dem Zielsystem geprüft werden.
- Kein RPM lokal erzeugt, weil `rpmbuild` fehlt. Der CI-Paketjob installiert die benötigten Werkzeuge.
- Keine Spielsitzung oder echten neuen EU-Mitschnitte. Nutzungsframes, Buff-Ende mit mehreren Wirkern, Unterbrechungen, Positionen und Cooldown-Zeiteinheit benötigen die in UPSTREAM_INSPIRATION.md beschriebene Ingame-Abnahme.

Es wurde kein Release veröffentlicht. Der offene Datenrechte-PR #33 und die bereits veröffentlichten v0.4.0-Assets wurden nicht geändert.

## Integration in 0.5.0

PR #33 entfernt die mitgelieferten unbestätigten Anbieterarchive. PR #35 beseitigt die doppelte Einstellungsabfrage und den zugehörigen Browser-Testfehler. Diese Änderungen werden vor der Veröffentlichung zusammen mit der Kampfanalyse durch die CI geprüft. Die obigen Zahlen dokumentieren die frühere Einzelabnahme, nicht die neue integrierte Testzahl.
