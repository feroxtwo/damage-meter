# Release-Abnahme: Skillkatalog und Symbole

Fortsetzung auf `main` nach `d93f875` (einschließlich Finalprüfung PR #6 und anschließender Fehlerkorrekturen/PNG-Diagramme). Die vorherigen geprüften Funktionen werden übernommen. Dieser Bericht bewertet ausschließlich die neue DE/EN- und Symbolintegration. Keine selbständige Zusammenführung oder Veröffentlichung eines Release-Tags.

## Urteil

**Technisches GO für die DE/EN- und Symbolintegration.** Kein bekannter offener P0/P1-Fehler, der ohne reale Kampfdaten noch behoben werden könnte. Die vollständige PR-CI ist grün: [CI-Lauf 64](https://github.com/feroxtwo/damage-meter/actions/runs/37692901429) zum geprüften Programmstand `1589d518ad6962b0310f9b06fc7e7c6783d7ed9c`, geprüft am 7. Oktober 2026. Der abschließende Bericht ergänzt danach ausschließlich Dokumentation; die aktuellen Branchchecks sind im [offenen PR #9](https://github.com/feroxtwo/damage-meter/pull/9/checks) nachvollziehbar.

**Reale Ingame-Freigabe bleibt offen.** Genauigkeit und Identität der real gesendeten Skill-IDs bleiben ohne reale Kampfdaten unverifiziert. Der PR wird gegen `main` veröffentlicht, nicht selbst gemergt. Kein Release-Tag wird erstellt.

## Abnahmematrix

| Bereich | Nachweis | Ergebnis |
|---|---|---|
| Fortsetzungsstand | Main `d93f875`; vorherige Finalprüfung und PNG-Diagramme vorhanden | Übernommen, keine Wiederholung alter manueller Analysen |
| DE/EN-Hauptkatalog | 364 IDs, beide Namen, neun Klassen, 41 gekennzeichnete Community-Übersetzungen | Bestanden |
| Varianten | 4.212 eindeutige Alias-IDs; kein Zuordnen allein durch Rundung unbekannter IDs | Bestanden |
| Offline-Artwork | 353 Skills + neun Klassen; alle 362 Bilder decodiert, SHA-256 und Grenzen kontrolliert | Bestanden |
| API / Rückfall | Katalog-HTTP, WebP-Header/Signatur, 404 unbekannter Assets, unbekannte IDs | Bestanden (Rust) |
| Sprachwechsel / Messwerte | Gespeicherten Skill DE/EN neu beschriften; Schaden, Hits, DPS und Trefferzeitpunkte identisch | Bestanden (Rust + JS) |
| Sprache / Persistenz | CLI-Voreinstellung, geschützte Settings-Mutation, Speicherung, ungültige Sprache | Bestanden (Rust) |
| Skill-/Klassenanzeige | Tabelle, Trefferzeitlinie, Vergleiche, native/OBS-Klassenicons | Bestanden (JS, Browser und native CI) |
| Katalogsuche | Alias-ID, beide Sprachen, Faustkämpferfilter und 80-Zeilen-Seite | Bestanden (JS + Browser) |
| Exporte | Gewählte Namen, unveränderte Werte, anonymisierte Spieler, lokale Skill-/Klassenbilder an Canvas übergeben; PNG-Seiten behalten alle Skills | Bestanden (JS + Browser) |
| Rust-Regression | 83 Tests | Bestanden |
| Formatierung / Clippy | `cargo fmt --all --check`, `cargo clippy --locked --all-targets -- -D warnings` | Bestanden |
| Release-Build | `cargo build --release --locked` | Bestanden mit Rust 1.88.0 (frischer lokaler Build) und Stable (PR-CI) |
| Vollständige Browser-Regression | 35 Browserfälle + sechs JS-Präsentationsprüfungen; alle fünf Tabs bei 320/390 px, Sprachwechsel, XSS-Schutz und PNG-Auswertung | Bestanden in PR-CI |
| Headless / Validierung / Native / Installer / Pakete | Bestehende CI-Skripte; Artefakte erfolgreich erzeugt | Bestanden in PR-CI |
| Mindest-Rustversion | `cargo check --locked` mit Rust 1.88.0 | Bestanden in PR-CI |
| Git / frühere Änderungen | `f0227b6` ist Vorfahr; Änderungen gegen Main begrenzt auf die unten aufgeführten 23 Dateien; sauberer Arbeitsbaum beim Abschluss | Bestätigt |
| Echte Kampfdaten | Reale Skill-IDs, Pets/Effekte, offizielle deutsche Faustkämpfernamen, Genauigkeit | Offen: separate Ingame-Abnahme |

Der installierte Stable-Compiler meldete zunächst fehlende Winit-Linkersymbole; die erste Rust-1.88-Gegenprüfung traf auf ungültige Build-Metadaten. Ein vollständig frischer Release-Build mit Rust 1.88.0 in einem separaten Target-Verzeichnis bestand. Auch der Stable-Release-Build und die Headless-, Validierungs-, Native-, Installer- und Pakettests in der PR-CI bestanden. Keine Produktkonfiguration wurde für diesen Gegencheck geändert.

Chromium benötigt im lokalen Container einen Unix-Socket, dessen Erzeugung verweigert wird. Die automatische Genehmigungsprüfung hat auch eine Ausführung außerhalb dieser Einschränkung abgelehnt. Diese lokale Blockade wurde nicht als bestandener Browsertest gewertet. Die vollständige echte Browserprüfung wurde stattdessen erfolgreich in GitHub Actions ausgeführt.

Die neue Navigation erhielt einen Zeilenumbruch für schmale Displays. Der bestehende OBS-Sicherheitstest prüft nun ausschließlich erlaubte lokale Klassensymbole und weiterhin die sichere Darstellung bösartiger Spielernamen. Der neue Sprachtest stellt nach dem Leerzustand seine Spielerfixture wieder her und wartet auf den vollständig geöffneten Detaildialog; Messwert-, Übersetzungs- und Exportassertionen bleiben erhalten. Nach diesen letzten Korrekturen bestanden alle drei CI-Jobs. Bereits bestandene manuelle Abnahmen wurden nicht neu aufgerollt; die konfigurierte CI läuft bei Branchupdates automatisch.

## Geänderte Dateien

| Dateien | Verbesserung |
|---|---|
| `data/skills/catalog.json` | Hauptkatalog, DE/EN-Namen, Klasse, Herkunft, exakte Aliase und Community-Kennzeichnung |
| `data/skills/icons.json`, `icons.bin`, `classes.rgba`, `NOTICE.md` | Lokales dedupliziertes Artwork, nachvollziehbare Herkunft/Checksummen, native Texturen |
| `src/skills.rs` | Zentrale Metadaten, sichere eingebettete Assets, historische Beschriftung ohne Veränderung der Messwerte |
| `src/main.rs` | Skillmodul registriert |
| `src/engine.rs` | Normalisierte, persistente Skill-Sprache mit CLI-Voreinstellung |
| `src/web.rs` | Katalog-/Asset-Endpunkte; gleiche Metadaten in Live, Run und historischen Detailantworten; API-Regression |
| `src/overlay.rs` | Einmal geladene Klassentexturen im nativen Meter |
| `web/index.html`, `web/skills.js` | Katalogtab, Such-/Klassenfilter, Pagination, Sprachwahl, gemeinsame Symbol-/Namensfunktionen |
| `web/enhancements.js` | DE/EN-Suche, Symboltabellen, Vergleiche/Zeitleisten und sprachabhängige Exporte |
| `web/qol.js` | Symbole in Paarvergleich und PNG-Berichten; lokale Bilder mit begrenzter Wartezeit |
| `web/overlay.html` | Klassensymbole im Browser-/OBS-Overlay |
| `scripts/import-skill-catalog.py` | Deterministische Aufbereitung lokal gesammelter Quellmanifeste; exakte Variantenauflösung |
| `scripts/test-skill-ui.cjs`, `scripts/test-web.cjs`, `package.json` | Ausführbare Präsentationsprüfungen und ergänzte echte Browserfälle |
| `README.md`, `docs/SKILL_CATALOG.md`, `docs/INGAME_ACCEPTANCE.md`, dieser Bericht | Bedienung, Umfang, Herkunft, Prüfnachweise und reale Restabnahme |

## Verbleibende Grenzen

- Katalog und Bilder sind ein Snapshot; Änderungen durch spätere Patches brauchen einen neuen Abgleich.
- Deutsche Faustkämpfernamen sind 41 Community-Übersetzungen, keine als offiziell ausgegebenen Quellnamen.
- Elf allgemeine Fähigkeiten haben kein Quell-Icon. Weitere Paket-/Effekt-IDs nutzen passende vorhandene Namen oder den ID-Rückfall; ohne sichere Zuordnung kein Skillbild.
- Alias-IDs und reale Klassen-/Pet-Zuordnungen müssen mit echten Aufnahmen überprüft werden. Bestehende Unsicherheiten zu Paketvollständigkeit, Buff-Entfernung, Overheal und nativer Plattformabnahme bleiben in der separaten Ingame-Liste.
- GitHub-CI bestätigt Browser und native X11-Regression; reales KDE/Wayland, gemischte Monitor-DPI und Spielszenen bleiben separat abzunehmen. Einzelne Parser-Skills behalten die bereits dokumentierten 32-Bit-Summengrenzen und die entsprechenden UI-Warnungen.
- Artwork bleibt Fremdmaterial von NCSOFT bzw. den jeweiligen Rechteinhabern; Herkunftsangaben und die Softwarelizenz gewähren keine eigenständige Artwork-Lizenz. Siehe `data/skills/NOTICE.md`.
