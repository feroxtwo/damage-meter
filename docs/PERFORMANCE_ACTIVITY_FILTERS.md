# DPS-Entwicklung nach Bereich und Expedition

Basis: `main` nach dem externen Merge von PR #17, Commit `d6df049`; Version bleibt 0.3.1. Erweiterung auf `feature/activity-performance`, [PR #18](https://github.com/feroxtwo/damage-meter/pull/18).

Die vorhandene Statistik „Wie entwickelt sich meine DPS?“ hat jetzt eine Auswahlhierarchie:

**Bereich → Expedition/Gebiet mit Schwierigkeit → gesamte Expedition oder einzelner Boss → Versuch/Run.**

Bereiche: Feldbosse, Open World, Expeditionen, Secret Dungeons, Nightmare, Transzendenz, Sanctuary/Raids, tägliche Dungeons und nicht zugeordnete Daten. „Alle Bereiche“ erhält den bisherigen Einstieg. Nach Auswahl einer konkreten Expedition steht „Gesamte Expedition“ vor deren Bossen. Normal, Schwer und einzelne Stufen bleiben separate Gebiets-IDs und separate Verlaufskohorten. Der vorhandene Charakterfilter, letzte 20/50/alle Punkte, Zeitregler, Berichtssprung und die fachlichen Grenzen der persönlichen Einordnung gelten weiter.

## Zwei verschiedene Messgrößen

- **Einzelner Boss:** unveränderte gespeicherte eigene DPS dieses Versuchs.
- **Gesamte Expedition / gesamter Run:** eigener erfasster Schaden geteilt durch die gemeinsame Summe erfasster Kampfzeitfenster, mindestens eine Sekunde je Kampf, ohne Training. Fehlende Teilnahme zählt als null Schaden. Wege und Pausen zählen nicht zur DPS. Das entspricht der bereits vorhandenen Run-Detailauswertung und ist kein einfacher Mittelwert einzelner Boss-DPS.

Nur beendete gespeicherte Runs erscheinen in der Gesamtentwicklung. Ein beendeter Run ist kein Nachweis, dass alle Bosse besiegt oder sämtliche Kämpfe vollständig erfasst wurden. Unterschiedliche aufgezeichnete Kampfzusammenstellungen können die Gesamt-DPS beeinflussen. Der Hinweis steht unmittelbar bei der Grafik. „Run öffnen“ führt zur vorhandenen Run-Auswertung, einzelne Versuche weiterhin zum Kampfbericht. Rote Punkte bedeuten erfassten eigenen Tod, keine Wipe-Bewertung.

## Zuordnung und Datenlücken

Bekannte Expeditions- und Transzendenz-IDs verwenden ergänzte Metadaten im bestehenden Dungeonkatalog. Referenz: [veröffentlichte Client-Dungeon-Daten](https://aion2.app/db/dungeons), abgerufen 8. Oktober 2026; die [NCSOFT-Mitteilung zu den getrennten Inhalten](https://about.ncsoft.com/en/news/article/aion2_update_260706) bestätigt die unterschiedlichen Modi. Vorhandene Namen werden nicht neu übersetzt. Nicht belegte Zuordnungen, darunter zusätzliche unbekannte Instanzen, bleiben ausdrücklich offen.

Bei gespeicherter Instanz-ID 0 unterscheiden vorhandene NPC-Daten bekannte Boss-NPCs und normale Gegner. Das ist eine Einordnung aus gespeichertem Kontext, keine unabhängige Bestätigung vollständiger Gebietserfassung. Fehlende IDs/NPC-Informationen erhalten keine erfundene Kategorie. Gleichnamige Zieltypen bleiben über ihre Mob- und Gebiets-IDs getrennt.

**„Bereich selbst zuordnen“** erlaubt eine eigene Einordnung für eine stabile Gebiets-ID oder, ohne Instanz, eine bekannte Mob-ID. So lassen sich auch noch nicht automatisch erkannte Secret-Dungeon- oder Nightmare-Aufzeichnungen gezielt filtern und Fehlzuordnungen korrigieren. Die Zuordnung gilt nur in diesem Browser und bleibt bei einem Reload erhalten. Nach dem Zuordnen wird direkt der gewählte Bereich angezeigt. Sie ändert weder Messwerte noch gespeicherte Kampf-/Run-Daten. „Automatisch / nicht zugeordnet“ entfernt sie wieder. Ohne stabile ID ist keine breite Zuordnung möglich. Kategorien ohne passende Daten zeigen einen verständlichen Leerzustand und deaktivieren den Berichtssprung.

## Technische Umsetzung und Prüfung

Die vorhandene Boss-History-API ergänzt Gebietsname, Kategorie, Scope und Mob-ID. Die neue read-only API `/api/stats/run-history?character=...` aggregiert vorhandene Runs und Kampfzeilen, ohne Schemaänderung oder neue Speicherung. Beide Antworten werden gemeinsam übernommen; verspätete Antworten dürfen einen neueren Charakterfilter nicht überschreiben. Filterwechsel mischen weder Schwierigkeiten noch Gesamt- und Boss-DPS. Manuelle Zuordnungen speichern nur Kategorie-Metadaten im Browser.

### Technische Abnahmematrix

| Prüfung | Ergebnis | Evidenz |
| --- | --- | --- |
| Rust-Aggregation und bestehende Regressionen | 95/95 bestanden; aktive Runs/Training ausgeschlossen, fehlende Teilnahme 0; Run-DPS entspricht den Run-Details; unbekannte Grenzmetadaten bleiben unbekannt | [CI #142](https://github.com/feroxtwo/damage-meter/actions/runs/37831291939), `33cf1c0` |
| Rust 1.88, Formatierung und Clippy | bestanden | CI #142 |
| Release-Build, API/Persistenz und Replay-Validierung | bestanden | CI #142 |
| Native X11-/Installer-/Paketprüfungen | bestanden, 60/100/150/200/250 %; bestehendes Overlay unverändert durch diese Erweiterung | CI #142 |
| Browser, einschließlich neuer Filterhierarchie | 53/53 Browserchecks bestanden, einschließlich finaler Run-Bezeichnungen | [CI #146](https://github.com/feroxtwo/damage-meter/actions/runs/37832054315), `edbe042` |
| Desktop/Mobil und Screenshots | 1440/320 px ohne Seitenüberbreite; Bilder visuell geprüft | unten |
| Reale Modi-/Gebiets- und Ingame-Abnahme | offen | separate [Ingame-Abnahmeliste](INGAME-PREMIUM-ACCEPTANCE.md) |

Die neue Browserprüfung deckt Gesamt-Run/Boss, Normal/Schwer, gleichnamige Ziele mit unterschiedlichen Mob-IDs, Umordnung der Antwort, Feldboss/Open World, leere Kategorien und Zuordnung/Reload/Entfernen ab. Die Suite enthält außerdem 58 Insight-Assertions, 8 Skill-Präsentationschecks und 9 Chart-Checks. Anfangs gefundene Formatierungs-, Berichtssprung- und Pluralfehler wurden korrigiert; fehlgeschlagene Läufe gelten nicht als bestanden. Die finale reine Dokumentationsrevision verändert die geprüften Laufzeitdateien nicht.

### Sichtbare Änderungen

Die Bilder stammen aus echten Chromium-Renderings mit gekennzeichneten synthetischen Testdaten. Die Referenz zeigt die vor dieser Erweiterung vorhandene Statistik; ihre Daten unterscheiden sich vom gezielten Filter-Testfall. Ein einzelner vorhandener Run wird als Punkt gezeigt und erzeugt keine erfundene Entwicklung.

- [Vorherige Statistik](telemetry-images/review-statistics.png)
- [Gesamte Expedition, Desktop](telemetry-images/activity-expedition-total.png)
- [Einzelner Boss, Desktop](telemetry-images/activity-expedition-1440.png)
- [Einzelner Boss, 320 px](telemetry-images/activity-expedition-320.png)

**Technisches Urteil:** bereit zur Gegenprüfung. Keine bekannten offenen P0/P1-Fehler dieser Erweiterung; automatisierte und synthetische Prüfung belegt keine reale Protokollgenauigkeit.

## Geänderte Dateien

| Datei | Änderung |
| --- | --- |
| `web/index.html` | Bereichs-/Gebiets-/Scope-Filter, manuelle Zuordnung, Run-Berichtssprung und Messgrößenhinweise |
| `web/enhancements.css` | Responsive Filtergruppe und zurückhaltende Zuordnungsdetails |
| `data/i18n/dungeons/en.json`, `src/names.rs` | Belegte Aktivitätsmetadaten im bestehenden Katalog |
| `src/db.rs` | Stabile Zielgruppen und beendete Run-Verläufe; zwei zusätzliche Aggregations-/Identitätstests |
| `src/engine.rs` | Kategorie aus vorhandenen Instanz-/NPC-Daten |
| `src/web.rs` | Read-only Run-History-Endpunkt |
| `scripts/test-web.cjs` | Kategorie-/Expeditions-/Bossauswahl, Berichtssprung, unbekannte IDs, Persistenz und schmale Displays |
| `README.md`, Abnahmedokumentation und `docs/telemetry-images/activity-*.png` | Bedienung, Datenlücken, Testnachweise und echte Browserbilder |

Keine zusätzliche Oberfläche, keine neue Laufzeitabhängigkeit und keine Änderung der Messdatenspeicherung.

Reale Ingame-Abnahme bleibt offen: korrekte Gebietserkennung, tatsächliche Modi und Zuordnung anhand echter Pakete sowie Übereinstimmung mit einer unabhängigen Messreferenz. Überheal, Shields, rDPS oder nicht erfasste Ereignisse werden daraus nicht abgeleitet.

## Unabhängige Gegenprüfung von PR #18

Verglichen mit `main` (`d6df049`) und dem ursprünglichen PR-Head `4ee050e`, ohne Merge. Die bestehende Run-Formel, Ausschluss von Training/aktiven Runs, getrennte Gebiets-/Mob-IDs und Berichtssprünge waren korrekt. Ein gespeicherter Run bleibt ausdrücklich kein Beleg für eine vollständige Expedition oder einen Kill.

Korrekturen:

- Alte History-Einträge mit `NULL` als Gebiets-ID behalten diese fehlende Information. Sie fallen nicht mehr mit expliziter ID `0` zusammen und bekommen keine gebietslose Mob-Zuordnung. Eine fehlende ID wird nicht aus dem Bossnamen geraten.
- Beim Laden einer anderen Charakterhistorie werden alte Kennzahlen, Diagramm und Berichtshandler sofort entfernt. Beide Antworten müssen zur aktuellen Anfrage und zum aktuellen Charakter passen. Fehler zeigen einen verständlichen Wiederholhinweis; neue Auswahlfelder bleiben während des Ladens gesperrt.
- Bereich, konkrete Expedition und Gesamt/Boss werden lokal gespeichert und nach Reload nur wiederhergestellt, wenn sie noch existieren. Entfallene Ziele erhalten eine gültige Alternative; entfallene gespeicherte Kategorien fallen bei vorhandenen Daten auf alle Bereiche zurück. Beschädigter/gesperrter Browserspeicher bleibt ohne Auswirkung auf Messdaten.
- Expeditionen verlangen eine konkrete Expedition vor der Auswertung. Bei Feldboss/Open World mit ausschließlich ID `0` entfällt das Gebietsfeld. Gleiche Gebietsnamen werden durch Roh-IDs unterscheidbar; fehlende Schwierigkeit heißt ausdrücklich „Schwierigkeit unbekannt“.
- Auf 320 px teilen sich Bereich und Zeitraum eine Zeile; die Auswertung folgt auf die Expedition. Die Zuordnung steht als geschlossene Detailansicht hinter der Analyse. 1440, 768 und 320 px wurden auf Überlauf und visuell geprüft. Der Browsertest sichert zusätzlich die kompakte Filterhöhe und gemeinsame erste Zeile ab.
- Die Zuordnungsdetails nennen Gebiets-/Ziel-ID, lokale Browsergeltung und unveränderte Messwerte. Freie Namen oder frei eingegebene IDs bietet diese Oberfläche nicht an; Namen/Unicode werden weiterhin escaped dargestellt.

Zusätzliche Regressionen: fehlende Gebiets-ID gegenüber ID `0`; getrennte Normal/Schwer/unbekannte Runs mit wiederholtem Boss, Nullschaden, kurzen Zeitfenstern, abweichender Speicherreihenfolge und fehlender Teilnahme; Auswahlpersistenz/entfallene Ziele; verspätete Charakterantworten und gesperrte Berichtssprünge; gleiche Gebietsnamen mit anderen IDs; beschädigter und gesperrter localStorage. Bestehende Tests für Zuordnungsänderung/-entfernung, leere Kategorien und Run-/Kampfberichtssprünge bleiben erhalten.

Lokale Prüfung der Gegenrevision: **97 Rust-Tests, 58 Browserprüfungen, 58 Insight-Assertions, 8 Skill- und 9 Chart-Prüfungen**, fmt, Clippy mit `-D warnings`, Release-Build sowie API-/Persistenz-/Replay-Integration und das echte Release-Dashboard. Die vollständige GitHub-CI prüft zusätzlich Rust 1.88, Paketierung und native X11-Abnahme am aktualisierten Head.

Die 14 vom Nutzer bereitgestellten Aufzeichnungen wurden ausschließlich lokal abgespielt und gegen einen separaten `main`-Build verglichen. Alle gemeldeten Werte stimmen überein; nur ungeordnete Ziel-/Akteur-/Skilllisten und Hit-Zeitstempel wurden für den Vergleich sortiert, die Reihenfolge der Analytics-Zeitpunkte blieb erhalten. Eine v1-Datei hat eingeschränkte Metadaten, eine v3-Datei enthält drei erkannte Datenlücken. Rohaufzeichnungen und personenbezogene Replayberichte werden nicht ins Repository aufgenommen. Replaygleichheit ist kein unabhängiger Beleg für reale Boss-/Gebietszuordnung oder vollständige Bossabdeckung.

Urteil der Gegenprüfung: **PR #18 MIT KLEINEN KORREKTUREN GUT**. Reale Protokoll-/Gebietsgenauigkeit, Kill-/Wipe-Auslegung und menschliche KDE/Wayland-Abnahme bleiben die bereits dokumentierten offenen Abnahmepunkte.

## Dungeon-Gesamtstatistik mit Skills und Damageverlauf

In der Run-Auswertung ergänzt **Dungeon-Gesamtstatistik** die bestehenden Gesamtranglisten und Einzelberichte. Einstieg: **Runs → Run öffnen**, oder **Statistik → Gesamte Expedition → Run öffnen**. Die Ansicht zeigt wahlweise die gesamte Gruppe oder einen einzelnen Spieler; bei genau einem passenden eigenen Spieler startet sie mit dessen Auswertung.

- **Schaden, DPS, Kampfzeit und Kampfzahl** gelten für alle gespeicherten Kämpfe des ausgewählten Runs ohne Training. Erfasste Mobs und wiederholte Bossversuche gehören ebenfalls dazu. Fehlende Teilnahme bedeutet 0 Schaden über das gemeinsame Zeitfenster, mindestens eine Sekunde je Kampf. Wege/Pausen gehören nicht zur Kampfzeit. Nicht gespeicherte Kämpfe werden nicht ergänzt.
- **Schadensskills** werden über Skill-ID und DoT-Typ summiert. Suchbare/sortierbare Tabellen behalten Namen, Sprache und Icons. Skill-DPS verwendet die gesamte gemeinsame Kampfzeit; Trefferquoten werden mit Trefferzahlen gewichtet. Unbekannte Skill-IDs werden nicht anhand gleicher Namen zusammengelegt. Unbekannte Quoten, Min/Max und optionale Zähler bleiben unbekannt. Heilung seit Parser-Reset wird nicht zu einer vermeintlich gesicherten Dungeon-Heilsumme addiert.
- **Damageverlauf** bietet DPS mit 5s-Glättung oder einzelnen Intervallen sowie Gesamtschaden, Zeitpunktwahl und Sprung zum konkret ausgewählten Kampfbericht. Die Zeitachse reiht gespeicherte Kampfzeitfenster chronologisch aneinander. Linien und Glättung beginnen an Kampfgrenzen und erkennbaren Aufzeichnungslücken neu. Fehlende Zeitreihen behalten leere Zeitfenster; ihre gemessenen Schaden-Summen bleiben in den Gesamtkennzahlen und in den Startwerten nachfolgender Kämpfe enthalten. Unvollständige Aufzeichnungen und Parser-Zahlengrenzen werden sichtbar genannt. Ein älterer Run ohne Zeitreihen bleibt über vorhandene Summen/Skills auswertbar, bekommt aber keinen erfundenen Verlauf.
- **Identität** verwendet Name, gespeicherten Server und Klasse statt wechselnder Actor-IDs. Gleichnamige Spieler verschiedener gespeicherter Server/Klassen bleiben getrennt; fehlende Serverdaten werden nicht ergänzt. Automatisch erzeugte Actor-Namen wie `#123` gelten nur innerhalb ihres Kampfes und werden nicht zu einer vermeintlich stabilen Spieleridentität zusammengezogen. Die Daten werden konsistent über die neue read-only API `/api/runs/{id}/analysis` geladen. Sie verwendet denselben Kampfdecoder wie Einzelberichte, ohne Schemaänderung oder zusätzliche Speicherung.
- **Bedienung** hält die allgemeine Kampf-/Mob-Bibliothek während einer geöffneten Run-Auswertung aus dem Weg. `← Zurück` stellt sie wieder her. `/` fokussiert die Skill-Suche des sichtbaren Run-Berichts; offene Kampfmodale haben weiterhin Vorrang. Bei Run-/Charakter-/Seitenwechsel dürfen alte Antworten die neue Analyse nicht überschreiben. Ladefehler bieten eine Wiederholung an. Bei Fenstergrößenwechseln wird das Diagramm für lesbare Achsen neu gezeichnet; Skill-Suche und gewählter Zeitpunkt bleiben erhalten.

Ergänzte Tests prüfen API/Decoder, Training-Ausschluss und andere Runs, leere/fehlende Runs, alte Datensätze ohne Skill-/Zeitreihen, gemeinsame Zeitfenster, null/fehlende Teilnahme, wechselnde Actor-IDs, gleiche Namen auf unterschiedlichen Servern, Skill-IDs/DoT/gewichtete Quoten, Messdaten-Unveränderlichkeit, Teilaufzeichnungen/Datenlücken und unterbrochene Kurven. Browserprüfungen decken Auswahl, Suche, Damage-/DPS-Verlauf, Berichtssprung, verspätete Antworten und 1440/768/320 px ab. Der neue Rechentest `scripts/test-run-analysis.cjs` läuft in `npm test` und damit in CI.

Lokale Prüfung der Dungeon-Erweiterung: 99 Rust-Tests, 60 Browserprüfungen, 30 numerische Run-Assertions, 58 Insight-Assertions, 8 Skill- und 9 Chart-Prüfungen sowie fmt/Clippy, Release und API-/Persistenz-/Dashboard-Integration. Die nachfolgende CI-Abnahme gehört zum neuen Feature-Head; die vorherige Abnahme bleibt als historischer Stand dokumentiert.
