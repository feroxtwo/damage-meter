# Release-Abnahme: Skillkatalog und Symbole

Fortsetzung auf `main` nach `d93f875` (einschließlich Finalprüfung PR #6 und anschließender Fehlerkorrekturen/PNG-Diagramme). Die vorherigen geprüften Funktionen werden übernommen. Dieser Bericht bewertet ausschließlich die neue DE/EN- und Symbolintegration. Keine selbständige Zusammenführung oder Veröffentlichung eines Release-Tags.

## Urteil

Technische Umsetzung zur PR-Abnahme vorbereitet. Kein bekannter zusätzlich eingeführter P0/P1-Fehler. Releasefreigabe für diese Erweiterung setzt grüne PR-CI voraus, insbesondere die vollständige Browser-Regression. Echte Ingame-Genauigkeit und Identität der real gesendeten Skill-IDs bleiben ohne reale Kampfdaten unverifiziert.

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
| Skill-/Klassenanzeige | Tabelle, Trefferzeitlinie, Vergleiche, native/OBS-Klassenicons implementiert | Logik geprüft; visuelle Browser-/Native-Regressionsabnahme in CI |
| Katalogsuche | Alias-ID, beide Sprachen, Faustkämpferfilter und 80-Zeilen-Seite | Bestanden (JS); zusätzlicher Browsertest ergänzt |
| Exporte | Gewählte Namen, unveränderte Werte, anonymisierte Spieler, lokale Skill-/Klassenbilder an Canvas übergeben | Bestanden (JS); vollständige PNG-/Browserchecks in CI |
| Rust-Regression | 83 Tests | Bestanden |
| Formatierung / Clippy | `cargo fmt --all --check`, `cargo clippy --locked --all-targets -- -D warnings` | Bestanden |
| Release-Build | `cargo build --release --locked` | Bestanden mit Rust 1.88.0, frischer Build in separatem Target-Verzeichnis |
| Vollständige Browser-Regression | Bestehende Suite + zwei neue DE/EN-/Katalogfälle | Lokal durch Chromium-Socketbeschränkung blockiert; CI erforderlich |
| Echte Kampfdaten | Reale Skill-IDs, Pets/Effekte, offizielle deutsche Faustkämpfernamen, Genauigkeit | Offen: separate Ingame-Abnahme |

Der installierte Stable-Compiler meldete zunächst fehlende Winit-Linkersymbole; die erste Rust-1.88-Gegenprüfung traf auf ungültige Build-Metadaten. Ein vollständig frischer Release-Build mit Rust 1.88.0 in einem separaten Target-Verzeichnis bestand. Keine Produktkonfiguration wurde für diesen Gegencheck geändert.

Chromium benötigt hier einen lokalen Unix-Socket, dessen Erzeugung verweigert wird. Die automatische Genehmigungsprüfung hat auch eine Ausführung außerhalb dieser Einschränkung abgelehnt. Das wurde nicht als bestandener Browsertest gewertet. Sechs davon unabhängige JS-Präsentationsprüfungen sind ausführbar und bestanden.

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
- Vollständige Browser-/Native-Regression im aktuellen lokalen Container nicht als bestanden behauptet. Die vorhandene frühere Abnahme ersetzt keine Prüfung der neuen Symbolanzeige.
