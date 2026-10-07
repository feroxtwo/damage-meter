# Releaseprüfung 0.3.1

Prüfdatum: 7. Oktober 2026. Ausgangspunkt: `main` bei **f0227b60611d09ac520118ad0d53b5b2f7bf0e44**. Änderungen auf `review/release-readiness`; kein Merge durch den Prüfer.

**Urteil: BEDINGT FREIGABEFÄHIG als Linux-Releasekandidat.** Die geprüften Softwarepfade und definierten synthetischen Messfälle bestehen. Reale AION-2-Genauigkeit, Ziel-Desktop/Spiel-Fensterverhalten und tatsächliche Paketinstallation bleiben offen. Keine allgemeine Genauigkeitsquote und keine Aussage „Ingame vollständig geprüft“.

## Ausgangsstand und Architektur

| Punkt | Tatsächlich vorhanden |
|---|---|
| Plattform | Native Linux x86_64 Binary; Fedora/KDE/Wayland als Ziel. Kein Windows-/macOS-Port. Browserdashboard auch vom Zweitrechner bei bewusstem LAN-Bind |
| Technologien | Rust Edition 2024, MSRV 1.88, eframe/egui 0.32, Axum 0.8/Tokio, rusqlite 0.37 mit gebündeltem SQLite, eingebettetes HTML/CSS/Vanilla-JS |
| Parser | GPL-3.0 A2Tools 2.0.52, fester Commit d3cf6f92533721939f4b4454d163c6d1dd820666, ohne Tauri-Hälfte. Namensdaten in data/ |
| Erfassung | AF_PACKET/CAP_NET_RAW, IPv4/IPv6 TCP, Dispatcher identifiziert beide IPs/Ports/Interface, TCP-Reordering und Duplikaterkennung |
| Datenpfad | capture.rs → dispatcher.rs/tcp.rs → Upstream-Parser → engine.rs → overlay.rs und web.rs; db.rs speichert Runs, Mitglieder, Kämpfe, Effekte, Annotationen und Analytics |
| Threads | Capture, Parser, 500-ms-Heartbeat, zwei HTTP-Runtime-Worker. Shutdown stoppt Verarbeitung, speichert, beendet Run und wartet auf Threads |
| Grenzen | 8192 Queue-Einträge; 32 Kandidaten; 1 MiB je Kandidat; 2 MiB/2048 TCP-Segmente; Parser hat eigene Aggregationsgrenzen |
| Start | Cargo/Installer oder Paket; Dashboard standardmäßig 127.0.0.1:8787; Portprüfung vor Threads; --no-overlay und --x11 verfügbar |
| Build / Tests | cargo --locked, fmt/clippy; Rust-/HTTP-/Replaytests, Playwright Chromium, Python CLI-/Referenztests; Node nur Entwicklung |
| Release | CI erzeugt Binary, Tar, DEB, optional RPM und SHA256SUMS; Tag-Workflow publiziert Releases; Version 0.3.1 |
| Installation | Benutzerinstaller mit setcap/KWin/Shortcuts; DEB postinst und RPM file capabilities. Datenbank bleibt im Benutzerverzeichnis |
| Updates | Explizite GitHub-Releaseabfrage, feste URL, Antwortlimit/Timeout, keine automatischen Executable-Downloads; kein Auth-/Cloud-/Telemetry-Dienst |
| Dokumentation | README, historische TECHNICAL_REVIEW/COMBAT_ANALYSIS/QOL_0.3.0, diese Prüfung, separate INGAME_ACCEPTANCE |

Baseline: **64 eigene Rusttests und 25 Browserchecks** bestanden. Native Skalierung anschließend im gestarteten Fenster als Fehler reproduziert. Historische Review-Dokumente sind ausdrücklich als frühere Stände gekennzeichnet.

## Befunde und umgesetzte Maßnahmen

P0: Im geprüften Anwendungsumfang kein zusätzlicher allgemeiner Releaseblocker reproduziert. Offene Ingame-Abnahme ist eine Freigabebedingung, keine bestandene Prüfung.

| Priorität | Befund / Reproduktion | Umsetzung | Nachweis |
|---|---|---|---|
| P1 | Frische Ubuntu-CI startet X11 nicht: dynamisch geladene libxkbcommon-x11 fehlt und winit panikt | DEB/RPM/CI-Abhängigkeit ergänzt, X11-Preflight mit konkreter Installhilfe | Reproduktion im GitHub-Runner, lokale Startchecks, Paketmetadata und erneute CI |
| P1 | Reguläres Ende vor periodischem Snapshot verliert kurze Versuche; Menü nutzte process::exit | Gemeinsamer geordneter Shutdown für GUI, SIGTERM und Ctrl-C, forced Snapshot, Run-Ende, Thread-Cleanup | Engine-Shutdown-Test, echte Binary mit SIGTERM und Neustart |
| P1 | Unvollständiger HTTP-Request kann graceful Serverende unbegrenzt aufhalten | Begrenztes Drain-Fenster, danach HTTP-Task abbrechen | Headless-Test mit absichtlich nicht abgeschicktem Header |
| P1 | Fenstermaß hängt nach Scalewechsel einen Zoom hinterher; Position kann mit Zoom wandern | Tatsächlich angewendeten egui-Zoom zur Größenänderung verwenden; Position unabhängig von UI-Zoom speichern | Echtes X11-Fenster: 360/540/720/900/216 px, konstante Koordinaten, Neustart |
| P1 | Kurzzeit-DPS in Browser/Run weichen vom 1-s-Minimum in Historie ab | Gemeinsame Mindestzeit; Run-Summen über dieselben Kampfzeitfenster einschließlich Nichtteilnahme | Unabhängiger Sollfall und 500-ms-/Mehrkampf-Test |
| P1 | Gespeicherte Gesamtsumme übernimmt 32-Bit-FightRecord, kann bei mehreren Skills >2.147 Mrd falsch sein | DB-Gesamtsumme aus 64-Bit-Skillsummen; nicht aufgeführte Skill-Akteure erhalten | 3-Mrd-Synthese; Grenze einzelner Upstream-Skills bleibt ausdrücklich sichtbar |
| P1 | Live-DPS-Karten zeigen beim Heilungsmodus weiter Schaden; natives Headeraggregat ebenso | Labels/Total/Selbst/Gruppe verwenden den ausgewählten Modus | Browserchecks, Codeprüfung des nativen Headers |
| P1 | Löschen aktiver Runs ermöglicht Verwaisung der laufenden Zuordnung | UI sperrt Aktion; API/DB lassen nur beendete Runs löschen | DB- und API-Regression |
| P2 | Statistiken nennen erfasste Versuche „Kills“ und mischen Schwierigkeiten | Versuche, Dungeon/Schwierigkeit und Charakter/Klasse getrennt gruppieren | DB-Test gleicher Boss in zwei Schwierigkeiten |
| P2 | Namen mit ; oder Pipe zerlegen Run-Mitglieder | SQLite-JSON statt delimiterbasierter GROUP_CONCAT | Unicode-/Delimiter-Test |
| P2 | Jede Live-Aktualisierung klont volle Trefferlisten | Live nutzt Aggregate und Heilungssnapshot; Detaildaten nur bei Bedarf | Live-/Healing-Solltest, Codeprüfung |
| P2 | Kurvenbudget nur je Ziel, Front-Remove teuer; gekürzte Kurve kann falschen Burst/ersten Peak erzeugen | VecDeque und gemeinsames 14.400-Punkte-Budget; fehlende Baseline nicht als null auslegen | 32-Ziele-Test und Browser-Test ohne erfundenen Startpeak |
| P2 | Viele frische Effekte wachsen trotz altersbasierter Prune-Regel | Max. 256 Ziele, 64 Effekt/Caster-Paare je Ziel, 128 Intervalle je Paar und 16.384 insgesamt; Kürzung markiert | Belastungstest, Zähler/Prune und Partial-Metadaten |
| P2 | Unnötig große Diagramm-DOMs und eager Detailtabellen | Höchstens 601 Kurvenpunkte, begrenzte Marker, Reports laden Details erst beim Öffnen | 100.000-Punkte- und Lazy-Detail-Browsercheck |
| P2 | Installer schreibt laufendes Binary direkt und Desktop-Exec scheitert an Sonderpfaden | Vorbereitete temporäre Datei mit Capability atomar ersetzen; Desktop-Quoting; gemeinsamer Installpfad, Shortcut-Prefix/Exec ebenfalls korrigiert | Frische Installation, Upgrade bei laufender Binary, setcap-Fehler simuliert, Sonderzeichenpfad |
| P2 | Archive haben README-Links ohne enthaltene Dokumente; Manifest kann alte Pakete einschließen | Docs in Tar/DEB/RPM, Manifest nur aktuelle Version | Extraktions-/Metadaten-/Checksumprüfung |
| P2 | Korrupt gespeicherte Settings bypassieren API-Normalisierung | Gemeinsame Normalisierung bei Load, Profile, API und Menü; ungültige Zahlen/Koordinaten verwerfen | Settings- und Restart-Test |
| P2 | Falsche Trefferquoten bei ungültiger Trefferzahl; fehlende Durchschnitt/Skill-DPS-Werte | Gültige Nenner, unbekannt als null; Durchschnitt, Anteil, Skill-DPS/HPS | Unabhängiger Solltest plus Null-/Großzahltest |
| P3 | Skilltabellen zeigen alle Sondermerkmale gleichzeitig | Kleine Haupttabelle, optionale Merkmale, Suche, Sortierung und exakte Zahlentooltips | Chromium plus visuelle Detailprüfung |
| P3 | Live-Refresh verliert Tastaturfokus | DOM nur bei Änderung ersetzen, Fokus über Actor-ID wiederherstellen | Fokus über Refresh getestet |
| P3 | Keine konkrete Erklärung fehlender Pakete / veralteter Anzeige | Berechtigungs-/Prozess-/Verbindungs-/Idle-Hilfe; Paketanzahl/letzter Empfang; Diagnose mit Feld-Allowlist | Echte Leeransicht, Diagnose-Privacytest, Fehlerzustände |
| P3 | Fehlende Filterrücksetzung, Labels, Defaultprivacy, kompakte Zeilen | Filter zurücksetzen, Datum validieren, Eingabelabels, anonym standardmäßig, tatsächlich reduzierte Row-Höhe | Browserprüfung und X11-Geometrie |

## Funktions- und Testmatrix

✅ tatsächlich ausgeführter Softwarepfad; ⚠️ synthetischer Mess-/Eventfall; 🔍 nur Codeprüfung; ⏳ Umgebung/Realaufnahme fehlt. Kombinierte Status beziehen sich auf unterschiedliche Teile derselben Zeile.

| Prüfbereich / vorhandene Funktion | Methode | Ergebnis | Status | Restrisiko |
|---|---|---|---|---|
| Start / Portkonflikt / Initialisierung | Release-Binary, echtes HTTP, belegter Port | sauberer Start/Fehler; vier Dashboardtabs | ✅ | Distribution/GPU anders |
| Ende / Neustart / Cleanup | SIGTERM, unvollständiger Request, Settings nach Restart; Engine mit kurzem Kampf | bestanden | ✅ ⚠️ | SIGKILL/Stromausfall kann keine letzte Speicherung durchführen |
| AF_PACKET / TCP / Dispatcher | definierte IPv4/IPv6-Payloads, Fragmente, Offloading, Signaturen, Flow-IDs | Tests bestanden | ⚠️ | echte VPN-/Proton-/Kernelpfade offen |
| Parser / unbekannte / doppelte / verspätete Daten | 123 Upstreamtests, eigene TCP-/Replaytests; abgeschnittene Dateien | Tests bestanden | ⚠️ | aktuelle Spielversion und komplette Eventabdeckung offen |
| Spieler / Party / Summons / Skills | Upstreamtests, eigene Heal-only-/Skills-/DB-Tests, Datenflussprüfung | definierte Fälle bestanden | ⚠️ 🔍 | reale Besitzer-/Charakter-/Klassenwechsel offen |
| Schaden / DPS / HPS / Dauer / Anteile | unabhängig berechnete fixe Werte durch echte Engine | konsistent bei definiertem Zeitmodell | ⚠️ | Healing ist seit Reset, keine Overheal-Abtrennung |
| Crit / Hits / Min / Max / Mittel / Skillrate | Treffer 100/300, 1 Crit; Null-Hits und große Zahlen | Sollwerte bestanden | ⚠️ | Beobachtungsabdeckung nicht bewiesen; Einzel-Skill-i32-Grenze |
| Boss-HP / Tod / Wiederbelebung | Monitor- und Upstreamtests, HP-Schätzung geprüft | definierte Fälle bestanden | ⚠️ 🔍 | echte HP-/Death-Pakete offen |
| Kampfende / Idle / Wipe / manueller Reset | Short-attempt-, failure-, training-exemption-, wipe-tests | bestandene Heuristiken; Save vor Reset | ⚠️ | echtes Wipe-/Zone-Protokoll; siehe Savefehlergrenze |
| Buffs / Debuffs / Uptime / Zeitlinie | synthetische Layouts/Refresh/Caster/Intervallunion, DB-Ziel-ID | bestanden | ⚠️ | Entfernung/Dispel nicht erfasst, Speichergrenzen markieren Partial |
| Training / Bestwerte | echte API, synthetische Uhr; ready/running/finish/interrupt | bestanden | ✅ ⚠️ | reale Puppe/wechselnde Ziele offen |
| Runs / Historie / Aggregation / Partner | DB-Fixtures und echte leere API, 5.000-Kampf-Historie | bestanden; Versuche statt Kills | ✅ ⚠️ | lange Sessions, reale Zuordnung offen |
| Suche / Filter / Sortierung / Pagination | Character/Unicode-Fälle, Daten/Queries, Chromium Requestrennen | bestanden | ✅ ⚠️ | sehr große DB nicht unbegrenzt schnell |
| Live-/Spieler-/Kampf-/Vergleichsdetails | Chromium mit expliziten Fixtures, echte Leeransichten | bestanden; Lazydetails/601-Punkte-Grenze | ✅ ⚠️ | gefüllte Ansichten nicht mit echten Kampflogs |
| Replay A2MCAP1/2/3 | Unit- und CLI-Fixtures inkl. Clock/Truncation/TCP-Dedupe | bestanden | ✅ ⚠️ | keine reale Aufnahme; v1 ohne volle TCP-Metadaten |
| Export Text/Chat/JSON/CSV/PNG | Downloads, PNG-Signatur, mehrseitige Skills, Privacy/Formelprüfung | bestanden | ✅ ⚠️ | Rohaufnahmen nicht anonymisiert; PNG-Textprüfung kein Bild-OCR |
| Profile / Settings / Migration | echte HTTP-/Binary-Tests, alte DB/defekte Werte, Restart | bestanden | ✅ ⚠️ | keine rückwirkende Neuberechnung alter Kampfwerte |
| Themes / kompakt / Layout / Fokus | Chromium 320/1440 px, DPR2, alle Themes, lange Namen, 24 Spieler/100 Skills | bestanden, Bilder visuell geprüft | ✅ ⚠️ | weitere Browser/Bildschirme offen |
| Natives Overlay / Scale / Position / Sichtbarkeit | echtes eframe-Fenster in Xvfb/software Mesa, Scale 0.6–2.5 und Restart | Geometrie/Settings/Ende bestanden | ✅ | keine realen Kampfreihen, kein Window-Manager |
| Always-on-top / Click-through / Hotkeys / Drag | egui-Commands, KWin-/Shortcut-Skripte gelesen | plausible Umsetzung | 🔍 ⏳ | KDE/Wayland/echtes Spiel/Fokus/Multimonitor fehlen |
| Performance / Ressourcen | Release-Kurzprobe 5.000 Kämpfe/500 Runs/4 Spieler, 140 Requests, 4 Worker | alle Requests erfolgreich, Messdaten unten | ✅ ⚠️ | kein stundenlanger Leak-/Eventratenbeweis |
| API / Security / Eingaben | Host/Port/Header, Grenzen, Escape/CSV, Pfade, feste Update-URL | Guards und Manipulationsfälle bestanden | ✅ 🔍 | bewusstes LAN-Bind ohne Auth; kein vollständiges CVE-Audit |
| Updates | Versions-/URLtest, Browserfixture; HTTP-Client gelesen | expliziter Check, keine Executable-Ausführung | ✅ 🔍 | reale neue Veröffentlichung/Upgrade auf Zieldistro offen |
| Packaging / Installer | Release-Tar/DEB/RPM erstellt, extrahiert/Metadaten/Checksums; Benutzerinstallerstubs | bestanden | ✅ | Paketmanager-Installation und echte setcap-Rechte hier nicht nachgewiesen |
| Autostart / Monitor-ID / manueller Resize | Codeinventar | nicht vorhanden; Größe über Scale/Rows, Position über Koordinaten | 🔍 | keine erfundenen Funktionen; Hardware-Abnahme bleibt offen |

## Unabhängige mathematische Sollwerte

Gemeinsames Ziel: Treffer bei 1.000 ms (Me:100, crit), 3.000 ms (Me:300), 11.000 ms (Other:600); Me heilt zweimal100. Unabhängig: Dauer=10.000 ms; Gesamt=1.000; Me=400/40 DPS/40 %; Other=600/60 DPS/60 %; Me-Heilung=200/20 HPS. Me-Skill:2 Treffer, 50 % beobachtete Crits, Mittel200, Min100, Max300, Skill-DPS40. Live, player_details, Replaybericht und gespeicherter Kampf stimmen in diesem Test überein. Weitere Leerlaufsekunden vergrößern die Ziel-Dauer nicht.

Kurzfall:500 ms wird auf1 s normiert. Runfall:0.5-s-Kampf +9-s-Kampf → gemeinsamer Nenner10 s, absent player zählt mit0. Großfall: zwei Skills mit je1.5 Mrd → gespeicherte Summe3 Mrd. **Keine Behauptung, ein einzelner Upstream-Skill könne unbegrenzt wachsen.**

Abweichende Kennzahlen sind beschriftet: Run-Liste=arithmetischer Durchschnitt der gespeicherten persönlichen Fight-DPS; Run-Detail=Summenschaden/gemeinsame Fightfenster; Burst=gleitende5 s; Kurve=beobachtete Intervall-DPS alle500 ms. Heilung ist seit Parser-Reset und nicht auf einzelne Targets isoliert. Exporte verwenden dieselben gespeicherten Spielerwerte; Browserfixtures testen Inhalt/Anonymisierung getrennt von Engine-Mathematik.

## Konkurrenzvergleich: dokumentierte Features, kein unabhängiger Accuracy-Benchmark

Primärquellen am Prüfdatum: [AionFlex Shop](https://aionflex.gg/shop), [A2Tools](https://a2tools.app/), [A2Tools Quellprojekt](https://github.com/taengu/A2Tools-DPS-Meter), [Grachy Aion2t](https://github.com/Grachy/aion2t-dps-meter). Weitere relevante Produkte: [Eclipse](https://github.com/Iota-Nine/Aion2-Eclipse), [SkeeveAN Aion-DPS-Meter](https://github.com/SkeeveAN/Aion-DPS-Meter), [RATmeter](https://github.com/Kuroukihime/AIon2-Dps-Meter). Marketinggenauigkeit, Bediengeschwindigkeit und grafische Qualität wurden nicht durch ausgeführte Konkurrenz-Binaries bestätigt. „Nicht belegt“ heißt nicht „fehlt“. Proprietäre Assets und fremder Code wurden für diese Änderungen nicht übernommen.

| Funktion | Unser Meter nach Prüfung | AionFlex | A2Tools | Grachy Aion2t | Bewertung gegenüber dokumentierten Angeboten | Maßnahme / vorher → nachher |
|---|---|---|---|---|---|---|
| Hauptansicht / Gruppenliste / DPS/HPS / Share | lokal, Modi, KPI, Klassenfarben | Overlay/Gruppenwerte, Tarifgrenzen | Liveparty/Analyse | Party-DPS | GLEICHWERTIG im lokalen Grundumfang | KPI vorher inkonsistent → passender Modus |
| Skill-/Kampfdetails / Crit / Hits | Tabellen, Details, Zeitlinien; unbekannt ausdrücklich | Skillanalyse | ausführliche Analyse | Details/Timelines | GLEICHWERTIG für belegte Kerninfos | breite Tabelle → Suche/Sortierung/optional/Ø/Skillrate |
| Buff-/Debuff-Erfassung / Support | partielle Dauerbeobachtung, kein Shield-/rDPS-Modell | Defence im Premiumumfang | Analysen dokumentiert, Vollständigkeit unbestimmt | Buffanalyse dokumentiert | SCHWÄCHER bei tiefen Supportmodellen | ehrlich beschriften, Ziel-ID fix, Speicherlimit; kein unbelegtes Effektmodell |
| Historie / Trends / Schwierigkeit | unbegrenzt lokale DB, Filter, Versuche, getrennte Tiers | tarifabhängig, auch Cloudtrends | Auto-History/Filter | History/Filter | GLEICHWERTIG lokal; FEHLT Cloudvergleich | gemischte Schwierigkeiten/falsche Kills → getrennte Versuche |
| Replay / Diagnose | lokale Metadatenaufnahme und CLI-Auswertung | öffentliche Featuretiefe nicht belegt | Logs/Analyse | Paketdiagnose | GLEICHWERTIG beim dokumentierten Analysezweck, direkte Replay-Gleichheit nicht belegt | Rohlogs lokal, privacy-sichere Diagnosecopy |
| Spielervergleich / Verlaufcharts | direkte Spieler und Fight-Vergleiche, Gruppenverlauf | Trends/Profiles | Analyse/Charts | Party-/Skilltimelines | GLEICHWERTIG für lokale Nutzung | Kurvenbudget/Lazyreports/fehlende Baseline korrigiert |
| Fenster / kompakt / Kontext / Hotkeys | natives Linux, KWin/CLI, OBS | Windows Overlay | Win/Linux Desktop | Windows Overlay | SCHWÄCHER bei validiertem Desktopkomfort | echtes Scale/Positionproblem behoben; Linuxspieltest offen |
| Export / Sharing / Anonymisierung | lokale Text/Chat/JSON/CSV/PNG, Namen standardmäßig anonym | opt-in Cloudsharing | Upload/Privacy/OBS | Screenshot/Text | GLEICHWERTIG lokale Exporte; FEHLT Clouddienst | unsafe default → anonymous; kein Cloudkonto nötig |
| Profile / Themes / Settings | lokale Profile, 3 Themes, kompakt | Themes, Characterprofiles | Themes, umfangreiche Settings/Sprachen | Settings | GLEICHWERTIG Anzeigeprofile; SCHWÄCHER Lokalisierungsumfang | settings normalisiert, echte compactrows |
| Updates / Onboarding / Status | manueller opt-in Check, Linuxpakete, konkrete Statushilfe | automatische Updates | distroabhängige Installation | Installation/Updates dokumentiert | SCHWÄCHER beim automatischen Updatekomfort | laufende Updates atomar, Docs enthalten, Paketstatus klar |
| Cloudleaderboard / Gearbuild-/Statrechner | nicht vorhanden | angeboten | Cloudanalyse | öffentlicher Umfang verschieden | NICHT SINNVOLL für diesen lokalen RC | Infrastruktur/Verifikation außerhalb Projektumfang |

Die Bezeichnungen bewerten den dokumentierten Funktionsumfang, keine universelle Produktsiegerliste. Unser Vorteil **BESSER für den konkreten lokalen Linux-Workflow** ist die Kombination ohne Account mit lokaler Historie, CLI-Replay und flexiblen lokalen Exporten; gegenüber vollständigen Clouddiensten ist das ein bewusster anderer Schwerpunkt, kein Qualitätsbeweis.

Eclipse dokumentiert zusätzlich getrennte DPS-/Heil-/Supportinformationen und mehr Analysefläche: übertragen wurde die klare Trennung „gemessen/unbekannt“, kein ungeprüftes Shield/Overheal-Modell. SkeeveAN dokumentiert sortierbare Listen, Detailzugriff und Copy/Hotkeys: passende Idee ist unsere Skill-Sortierung und keyboardfähige lokale Navigation. RATmeter dokumentiert leichtgewichtige Partywerte/Statwerkzeuge: Berechnungsrechner außerhalb sicher verifizierter Messung nicht übernommen.

Must-have umgesetzt: korrekte gemeinsame Zeitfenster, Save/Ende, Status, Scale und Datenintegrität. Sinnvoll umgesetzt: Skillsearch/sort, kleinere Tabellen, bounded charts, Diagnose, Installerfixes. Nice-to-have umgesetzt: Defaultprivacy, genaue Tooltips, Filterclear, compactrows und Fokus. Unnötig für diesen RC: Cloudpflicht, PvP-Produkterweiterung, neuer Frameworkstack, proprietäre Assets.

## Performance und Langzeitgrenzen

Release-Kurzprobe:5.000 synthetische Kämpfe,500 Runs,4 Spieler/Kampf;140 Requests mit4 parallelen Workern. Start57.63 ms; Gesamt3.67 s; gemessene CPU3.73 s. RSS20.184 →26.400 KiB und nach5 s Idle26.400 KiB; Idle-CPU0.00 s. Live-Median16.64 ms/P95127.23 ms, Such-Median138.13 ms/P95185.25 ms, Boss-Historie-Median177.09 ms/P95205.91 ms. Höchster beobachteter Request241.52 ms.

Diese Werte stammen aus einem gemeinsam genutzten Container, ohne Capture-Eventlast und ohne lange Dauer; keine Garantie gleicher Ergebnisse auf Zielhardware. Rohdaten: [benchmark-release.json](benchmark-release.json); wiederholbar über scripts/benchmark-history.py. Vergleich Debug vs Release ist **kein Vorher/Nachher-Optimierungsnachweis**.

Technisch begrenzt: Zusatzkurven, Buffintervalle, TCP/Candidates und Aufnahme-Rotation. Upstream-Trefferlisten/rohe Aggregate behalten eigene Grenzen; es gibt keine pauschale Behauptung „alle Speicherstrukturen sind nun konstant“. Keine CPU-/RAM-Verschlechterung aus einer mehrstündigen Spielsitzung ausgeschlossen. Sqlite-Abfragen laufen synchron mit gemeinsamem Mutex; unter weit größeren DBs kann HTTP warten. Erst reale Messungen rechtfertigen einen umfassenderen Worker-/Indexumbau.

## Tests, Build und Packaging

- Eigene Rust-/HTTP-/DB-/Replay-/Engine-Tests: **74 bestanden** (vorher64).
- Gesonderte Tests der gepinnten Upstream-Bibliothek: **123 bestanden**.
- Chromium-Regressionen: **31 bestanden** (vorher25); explizit synthetische API-Fixtures.
- Python Referenzvergleich: **4 bestanden**.
- fmt, Clippy mit -D warnings und Rust1.88 --locked Check: bestanden.
- Clean Release-Build in frischem /tmp/aion2-final-clean-build: bestanden; nach späteren kleinen Änderungen erneut Release gebaut.
- Echte Release-Binary: Headless/Port/Guards/Profile/Restart/Replay; gestartetes Produktionsdashboard ohne Fixtures; X11-Scale/Position/Restart; Benutzerinstaller mit Capability-Stubs: bestanden.
- Tar/DEB/RPM: Erstellung, Version/Architektur/GLIBC-Abhängigkeit, Inhalt, Docs, Capability-Manifeste und SHA256SUMS geprüft. Keine reale Paketmanagerinstallation in Fedora/Ubuntu/Bazzite behauptet.
- JavaScript ist Vanilla-JS ohne TypeScriptprojekt; Syntaxcheck und Chromium statt eines erfundenen Typecheck-Schritts. Keine unnötigen Runtime-Abhängigkeiten: nur vorhandenes Tokio mit signal-Feature und transitiver signal-hook-registry ergänzt.

Neue Rustregressionen decken gemeinsame Kurz-/Runfenster,64-Bit-Fightsumme/ungültige Quote, Memberdelimiter/aktive Runs, Tiertrennung/Versuche, shared curves/burst, strikte Effektgrenzen, Shutdown/Savefehler, defekte Settings, unabhängige Live/Detail/History/Replay-Mathematik und API-Limits ab. Neue Browserchecks decken Skillbedienung, KPI/Fokus, bounded/lazy/DPI, Privacydiagnose,24 lange Spielernamen/100 Skills/Zahlengrenze und Partialkurven ab. Neue Helfer: test-native.py, test-installer.py, test-real-web.cjs, benchmark-history.py, test-packages.py und test-native-dependencies.py. CI führt Installer-/Nativechecks zusätzlich aus. Der erste Native-CI-Lauf fand die fehlende dynamische X11-Bibliothek. Der korrigierte Code- und Teststand bei `25c438700d303d33d507da4e9deabdfd5a6ab3af` besteht inzwischen auf dem frischen GitHub-Runner vollständig: [CI 37611751972](https://github.com/feroxtwo/damage-meter/actions/runs/37611751972), Jobs `build`, `web` und `msrv` jeweils erfolgreich. Auch Headless-, Native-Abhängigkeits-, Installer-, Native-Geometrie- und Paketprüfungen sind dort ausdrücklich erfolgreich.

## Zweite UX-Runde und Bilder

Echte Anwendung erneut gestartet: leere Live-, Run-, Statistik- und Settingsansicht geprüft, Settings über Produktions-API geschrieben; natives Overlay skaliert, aus-/eingeblendet, positioniert und neu gestartet. Gefüllte Analyse-/Vergleichsansichten mit klar getrennten synthetischen Fixtures geprüft. 320 px,1440 px,DPR2, drei Themes und kompakte Ansicht sind abgedeckt. Kein vollständiges Redesign, keine externen Schrift-/Bildassets.

In Runde2 zusätzlich korrigiert: tatsächlicher angewendeter Zoom statt gewünschtem Zoom; skalierungsunabhängige Position; optionales Detailrendering; Tastaturfokus bei Refresh; kein falscher erster Intervallpeak nach Kürzung; Native-Header folgt der Kennzahl. Dialogscroll, Notizen/Exporte und Chartbreiten im bestehenden Theme erhalten.

| Bild | Herkunft |
|---|---|
| [Vorher: Dashboard](review-images/before-dashboard.png) / [Nachher: Dashboard](review-images/after-dashboard.png) | gleiche synthetische Fixture, keine echten Kämpfe |
| [Vorher: Vergleich](review-images/before-comparison.png) / [Nachher: Vergleich](review-images/after-comparison.png) | synthetische Spielerwerte; Bildthemes können wegen Testablauf differieren |
| [Skilldetails](review-images/after-skills.png), [Runs](review-images/after-runs.png), [Settings](review-images/after-settings.png) | Browserfixture, echte Layout-/Interaktionstests |
| [Produktionsdashboard leer](review-images/actual-live-empty.png), [Statistik leer](review-images/actual-stats-empty.png) | echte Binary und API, keine Dateninjektion |
| [Natives Overlay](review-images/actual-native.png), [200 %](review-images/actual-native-2x.png) | echtes eframe in virtuellem X11, erwarteter Capture-Fehler in dieser Umgebung, kein Spiel |

## Offene / bewusst nicht umgesetzte Punkte

1. **Reale Protokollkorrektheit:** keine realen Logs vorhanden. Pets, DoTs, HoTs, Overkill, Reflection, Partywechsel, Reconnect, Wipe und Currentpatch nicht final verifiziert. Separate [Ingame-Abnahme](INGAME_ACCEPTANCE.md).
2. **Desktop/Hardware:** KDE/Wayland/KWin, Fokus/Klickdurchleitung im Spiel, Alt-Tab, exklusives Vollbild, gemischte DPI und Multimonitor fehlen. Xvfb ohne WM beweist keine Always-on-top-Eigenschaft.
3. **Upstream-Zahlengrenze:** einzelner Skill-dmg und Kontext-Summary sind i32; Skills saturieren, zusammengefasste Contextcasts können begrenzt/falsch sein. Lokale 64-Bit-Fightsumme beseitigt nicht diese Parsergrenze. Warnung bei erkennbarer Grenze; breite Typmigration im externen Parser hat ohne reale Regressiondaten unverhältnismäßiges Risiko und wird nicht behauptet.
4. **Filesystem-Ausfälle:** manueller/automatischer Reset bleibt bei Savefehler stehen; normales Ende meldet Fehler. Upstream-Zonenreset hat einen void-before_reset-Callback und kann bei fehlgeschlagener Speicherung trotzdem Daten löschen; das wird jetzt sichtbar gemeldet. Eine verlustfreie Transaktion über externen Reset und ausgefallenes Filesystem benötigt einen größeren Parser-/Recoveryumbau und ist hier nicht als behoben ausgewiesen. Rohaufnahme ebenfalls nur hilfreich, wenn sie erfolgreich geschrieben wurde.
5. **Alte Historie:** bestehende gespeicherte Werte werden nicht ungefragt rückwirkend neu interpretiert. Neue Speichervorgänge verwenden korrigierte Summen. Bestehende Namen/Notizen/Profile bleiben erhalten.
6. **Messumfang:** keine effektive Heilung/Overheal-/Shield-/rDPS-/Dispel-Behauptung; Buff-Entfernung wird nicht beobachtet. Zusätzliche Modelle brauchen reale Paketbelege, sonst entstehen scheinpräzise Zahlen.
7. **Release/OS:** reale RPM/DEB/rpm-ostree-Installation, tatsächliche Capturecapability nach Upgrade und veröffentlichter Updateroundtrip fehlen. Lokale Build-GLIBC nicht automatisch Mindestversion für alle Distributionen; Paketmanifest bestimmt sie aus dem jeweiligen Binary.
8. **Security:** LAN-Bind bleibt ausdrücklich opt-in ohne Auth; lokale Guards sind getestet. Keine externe Credential-/Telemetrylogik hinzugefügt. Kein umfassendes Advisory/CVE-Audit behauptet.
9. **Außerhalb Umfang:** Cloudleaderboard, Gearrechner, PvP, Windowsport, Auto-Executable-Updater und identitäts-/monitorbasierte Placementarchitektur nicht eingeführt. Sie erfordern Infrastruktur/Plattformen oder unverhältnismäßige Regressionrisiken.

## Geänderte Dateien und PR

Core: Cargo.toml/Cargo.lock; src/analytics.rs, buffs.rs, capture.rs, db.rs, dispatcher.rs, engine.rs, main.rs, overlay.rs, web.rs. Oberfläche: web/index.html, enhancements.js/css und qol.js. Installation/Tests: scripts/install.sh, install-binary.sh, install-shortcuts.sh, desktop-exec.sh, package-linux.sh, test-headless.py, test-web.cjs sowie die sechs genannten neuen Test-/Benchmarkhelfer. CI: .github/workflows/ci.yml. Dokumentation: README, die drei historischen Dokumente, dieser Bericht, INGAME_ACCEPTANCE, Benchmark-JSON und review-images.

PR enthält die getesteten Änderungen; Freigabe/Merge bleibt beim Maintainer. [PR #6](https://github.com/feroxtwo/damage-meter/pull/6) enthält den veröffentlichten Stand. Keine Veröffentlichung eines endgültigen Releases durch diese Prüfung.


## Abschließende Fortsetzung und Release-Abnahme

Die Fortsetzung prüft den erhaltenen Arbeitsstand und die vorhandenen Nachweise, ohne die bestandene Softwareanalyse oder Testläufe zu wiederholen. Der Arbeitsbaum bei `25c4387` war sauber, alle sechs bisherigen Commits waren lokal und auf dem PR-Branch vorhanden. GitHub bestätigt den offenen, nicht gemergten und konfliktfrei gegen `main` gerichteten PR #6. Ausgangsbranch und PR-Basis stehen weiterhin bei `f0227b6`.

Seit dem letzten erfolgreichen CI-Lauf wurde für diesen Abschluss ausschließlich dieser Bericht ergänzt. Der getestete Anwendungs-, Installer-, Paket- und Testcode bleibt identisch. Deshalb ist lokal kein erneuter Regressionstest oder Release-Build erforderlich. Die automatisch durch den Dokumentationscommit gestartete GitHub-CI ist vom oben belegten erfolgreichen Code-Testlauf zu unterscheiden.

| Freigabekriterium | Ergebnis / Nachweis | Entscheidung |
|---|---|---|
| Bisherige Änderungen erhalten | Sauberer Git-Status, sechs Commits seit `f0227b6`, identischer lokaler und veröffentlichter Code-Head `25c4387` | Bestanden |
| Notwendige Regressionen nach letztem Codestand | Letzte CI einschließlich aller drei Jobs und nachträglicher X11-/Python-/Geometriekorrekturen erfolgreich | Bestanden, keine lokale Wiederholung |
| Release-Build und Softwaretests | 74 eigene Rusttests, 123 separat geprüfte Parsertests, 31 Chromiumchecks, 4 Referenztests; Release-Binary, Installer und Pakete geprüft | Bestanden im dokumentierten Umfang |
| Abschlussbericht / Funktionsmatrix / Dateiinventar | Dieser Bericht mit Befunden, Methoden, Grenzen, Bildern und vollständigem Inventar unten | Vollständig |
| Reale Ingame-Abnahmeliste | Separate `INGAME_ACCEPTANCE.md` mit Messprotokoll, 26 Szenarien und Offline-Referenzvergleich | Vollständig vorbereitet, Durchführung offen |
| P0/P1 bei funktionsfähiger Speicherung | Keine offenen reproduzierten P0/P1 im geprüften Softwarebetrieb; festgestellte P1-Befunde oben korrigiert und geprüft | Softwarekriterien bestanden |
| Ausfall der Speicherung | Reguläre Resets stoppen bei Savefehler. Externer Zonen-/Partyreset kann trotz Savefehler löschen; keine verlustfreie Wiederherstellung garantiert | Dokumentiertes Restrisiko, keine Freigabe für garantierte Verlustfreiheit |
| Reale Messgenauigkeit / Langzeitlast | Keine echten Kampfdaten und kein mehrstündiger Spielbetrieb vorhanden | Offen, keine Ingame-Freigabe |
| Zielsystem / Desktop / Paketinstallation | KDE/Wayland, Multimonitor, Spiel-Fokus, reale RPM/DEB/rpm-ostree-Installation und Capturecapability nach Upgrade fehlen | Offen, Abnahme auf Zielsystem nötig |
| Veröffentlichung / Merge | PR #6 veröffentlicht gegen `main`; kein Merge und kein endgültiger Release durch den Prüfer | Maintainer entscheidet |

**Finales Releaseurteil: BEDINGT FREIGABEFÄHIG als Linux-Releasekandidat 0.3.1.** Die Softwareprüfung ist abgeschlossen. Eine endgültige Ingame- oder Zielsystemfreigabe folgt erst aus der separaten realen Abnahme. Der Speicherfehlerpfad des externen Resets, i32-Grenzen im Parser, fehlende Overheal-/Dispel-Erfassung, Alt-Historie, freiwilliger LAN-Bind ohne Auth und fehlende mehrstündige Lastprüfung bleiben ausdrücklich die oben beschriebenen Restrisiken. Sie werden nicht durch erfolgreiche synthetische Tests als erledigt ausgegeben.

### Vollständiges Dateiinventar gegenüber `f0227b6`

47 geänderte oder neu hinzugefügte Dateien, einschließlich dieses Abschlussberichts. Binäre Bilddateien sind Vorschauen und keine Messdatennachweise.

```text
.github/workflows/ci.yml
Cargo.lock
Cargo.toml
README.md
docs/COMBAT_ANALYSIS.md
docs/INGAME_ACCEPTANCE.md
docs/QOL_0.3.0.md
docs/RELEASE_REVIEW_0.3.1.md
docs/TECHNICAL_REVIEW.md
docs/benchmark-release.json
docs/review-images/actual-live-empty.png
docs/review-images/actual-native-2x.png
docs/review-images/actual-native.png
docs/review-images/actual-stats-empty.png
docs/review-images/after-comparison.png
docs/review-images/after-dashboard.png
docs/review-images/after-runs.png
docs/review-images/after-settings.png
docs/review-images/after-skills.png
docs/review-images/before-comparison.png
docs/review-images/before-dashboard.png
scripts/benchmark-history.py
scripts/desktop-exec.sh
scripts/install-binary.sh
scripts/install-shortcuts.sh
scripts/install.sh
scripts/package-linux.sh
scripts/test-headless.py
scripts/test-installer.py
scripts/test-native-dependencies.py
scripts/test-native.py
scripts/test-packages.py
scripts/test-real-web.cjs
scripts/test-web.cjs
src/analytics.rs
src/buffs.rs
src/capture.rs
src/db.rs
src/dispatcher.rs
src/engine.rs
src/main.rs
src/overlay.rs
src/web.rs
web/enhancements.css
web/enhancements.js
web/index.html
web/qol.js
```
