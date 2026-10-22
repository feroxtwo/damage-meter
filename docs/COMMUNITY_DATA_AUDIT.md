# Prüfung der Community-Daten in v0.4.0

Prüfstand: 22.10.2026. Repository und Release-Commit: `5517139e8b777749aadf39f6671d246a1c2defc7`.
Zeitangaben der GitHub-Ereignisse unten sind UTC.

## Ergebnis und Beweisgrenzen

Für keines der sechs ausgelieferten Archive wurde in den geprüften Dateien,
PRs, Issue #22 oder den zugänglichen Betreiberinformationen eine ausdrückliche
Berechtigung zur Weitergabe der Statistikdaten über GitHub und Linux-Pakete
nachgewiesen. Alle sechs Archive enthalten `source.rights_confirmed: false`.
Das ist ein fehlender Freigabenachweis, keine abschließende Feststellung einer
Rechtsverletzung. Nicht öffentlich dokumentierte Vereinbarungen können hier
nicht ausgeschlossen werden.

Die Archive nennen Herkunfts-URLs, Erfassungszeiten vom 09.10.2026 und SHA-256
von angeblichen Rohantworten. Die Rohantworten selbst und eine Lizenz oder
Betreibererlaubnis sind nicht Teil dieser Archive. Ihre Herkunft ist deshalb
als **im Archiv deklarierte Provenienz**, nicht als unabhängig authentifizierte
Lieferung des Betreibers zu verstehen. Die Archiv-Hashes unten wurden dagegen
an Repository und ausgelieferten Binärdateien unabhängig nachgerechnet.

## Die sechs Archive

| Quelle | Zeilen | Deklarierte Herkunft / Inhalt | Lizenz-/Freigabebefund |
| --- | ---: | --- | --- |
| A2 Tools | 16 | `a2tools.app/stats`, inklusive EU/Vakron/KP-Filter. Klassenmediane und Band-/Filtermetadaten. | Die [GPL-Lizenz des Meter-Codes](https://github.com/taengu/A2Tools-DPS-Meter/blob/main/LICENSE) und die [Fremdmeter-Dokumentation](https://github.com/taengu/A2Tools-DPS-Meter/blob/main/docs/third-party-meters.md) betreffen Software bzw. konforme Uploads. Daraus ist keine Lizenz für den Statistikexport und dessen Weiterverteilung belegt. Betreiberfreigabe offen. |
| Aion DPS | 3 | `aiondps.com/api/bosses/vakron/leaderboard?game=aion2`. Run-Anzahl, bester Gruppen-iDPS, durchschnittliche Dauer. Keine Klassenmediane. | [MIT-Lizenz](https://github.com/SkeeveAN/Aion-DPS-Meter/blob/main/LICENSE) für Software und zugehörige Dokumentation nachgewiesen. Keine ausdrückliche Einbeziehung der separat betriebenen Statistikdaten nachgewiesen. Website-Bedingungen waren nicht vollständig abrufbar. |
| Abyss Logs | 25 | `api.abysslogs.com/v1/statistics?region=eu&period=all&fight=2300812`. Vakron-Klassenmediane, KP-Bänder und Quantile. | [Öffentliche Website](https://abysslogs.com/) beschreibt Teilen und Auswerten von Kämpfen. In den geprüften Informationen keine Datenlizenz oder Erlaubnis zur Drittweitergabe nachgewiesen. |
| Questlog | 8 | `questlog.gg/aion-2/en/armory/meta?region=eu`. Klassenindizes, keine absoluten DPS-Mediane. | [Nutzungsbedingungen](https://questlog.gg/aion-2/en/terms-of-service) waren nur teilweise über den Suchindex lesbar. Die indizierte Klausel zu automatisierten Zugriffen verlangt vorherige schriftliche Erlaubnis. Kein Nachweis einer solchen Erlaubnis oder einer Weitergabelizenz für dieses Archiv. Vollständige Bedingungen und archivbezogene Zustimmung bleiben zu prüfen. |
| JaMeter | 87 | `jameter.net/ranking/<class>` für neun Klassen. KP-Referenzwerte, Maximum/P90, ohne bestätigten Boss-/Schwierigkeitsbezug. | [Bedingungen v1.0 vom 23.08.2026](https://jameter.net/terms), insbesondere Art. 5: Rechte an selbst erzeugten Statistiken und Bildschirmgestaltung werden dem Betreiber zugeordnet. Art. 3 betrifft die freiwillige Übermittlung für dessen Aggregation/Anzeige. Keine ausdrückliche Lizenz zur Weitergabe dieses Archivs gefunden. |
| NotMeter | 9.089 | `notmeter.com/g/578d3695ce598fe2/data/notmeter-ranking.json.gz`. KR/TW-Klassenaggregate, Dungeon-/Boss-/KP-/Zeitraumfilter, DPS/nDPS-Quantile und Wochenvergleiche. | [Öffentliche Statistikseite](https://notmeter.com/) und ein erreichbarer Datenpfad belegen keine Weitergabelizenz. Keine passende Datenlizenz oder Betreiberfreigabe in den geprüften Informationen nachgewiesen. |

Gesamt: **9.328 Zeilen**, **2.041.181 Byte gzip**, **13.911.535 Byte JSON**.
Alle sechs verwenden `a2m-provider-observations-v1`. Enthalten sind numerische
Statistiken, Klassenschlüssel, Spiel-/Anbieter-IDs, Filter, Quellen, Zeitstempel
und Unsicherheitskennzeichen. Die untersuchten normalisierten Strukturen
enthalten keine individuellen Kampflogs oder Spielerprofile. Einzelne Felder
wie `characters` sind Aggregatzahlen, keine Charakterlisten.

Die Normalisierung läuft offline über `scripts/normalize-community.py`.
PR #31 beschrieb eine Wiederherstellung aus einer Sicherung. Sein damaliges
`prepare-embedded-references.py` ergänzte bei NotMeter `weekly_inferred` und
entfernte 44 mehrdeutige Zeilen. Diese Transformationen schaffen keine Datenrechte.

## Entscheidungsgeschichte und aktuelle Widersprüche

- [PR #28](https://github.com/feroxtwo/damage-meter/pull/28), Merge `72b9693` am 09.10.2026 um 18:01 UTC: keine ausgelieferten Anbieterarchive. Optionaler Builder und `--reference-db`, synthetische Tests. Die PR-Beschreibung nennt ausdrücklich die Entfernung mangels bestätigter Datenrechte.
- [PR #31](https://github.com/feroxtwo/damage-meter/pull/31), Merge `58a1ad0` am 12.10.2026 um 22:34 UTC: sechs `include_bytes!`-Einbindungen und Hintergrundimport bei jedem Start. Die Archivvalidierung prüfte nur Provenienz/Struktur, nicht `rights_confirmed`. Passende Beobachtungen konnten trotz `false` Vergleichswerte erzeugen.
- [Release v0.4.0](https://github.com/feroxtwo/damage-meter/releases/tag/v0.4.0): veröffentlicht am 12.10.2026 um 22:47:36 UTC, entsprechend 13.10.2026 um 00:47:36 in Deutschland. Enthält außerdem PR #32 zum manuell startbaren Release-Workflow.
- [Issue #22](https://github.com/feroxtwo/damage-meter/issues/22) war bei Prüfung offen, ohne Kommentare oder dokumentierte Freigaben. Auch die Diskussionen von #28 und #31 enthielten keine zusätzlichen Kommentare/Freigaben.
- `docs/COMMUNITY_DATA_SOURCES.md` beschrieb gleichzeitig integrierte Archive und offene Rechte. Es behauptete außerdem an anderer Stelle, Beobachtungen würden nie Kampf-Scores und die Archiv-API liefere immer `score_eligible: false`. Das entsprach nach #31 nicht mehr dem Code. Die Dokumentation wird mit dieser Änderung an das tatsächliche Verhalten angepasst.

## Prüfung der veröffentlichten Pakete

Alle drei offiziellen Pakete wurden heruntergeladen und ohne Installation
entpackt. Die Paket-SHA-256 stimmen mit den Digests der GitHub-Release-API überein.
Die ausführbaren Dateien sind identisch: **23.744.632 Byte**, SHA-256
`b81918bc0dbfff20ae4b576c85008f6b81adafb01429be8b2b4353fd5ab3b50c`.

| Paket | SHA-256 |
| --- | --- |
| `aion2-meter-0.4.0-1.x86_64.rpm` | `25fdaa7e835941befa0b8841aea08e9d86ae9e81f9104ba0b952f89a6ec19716` |
| `aion2-meter-0.4.0-linux-x86_64.tar.gz` | `2efba24e81d2ea54713b68897435f8b54597686ab0e56b1b390847797dd64cf9` |
| `aion2-meter_0.4.0_amd64.deb` | `2b8cc68fabc947e3df603bfff676f0e0025c4d6e6e1df6b38013a0675d5a63a0` |

Die folgenden gzip-Dateien sind in jedem ELF-Binary vollständig und bytegleich
zum Tag `v0.4.0` enthalten. Offsets sind nullbasiert und dezimal.

| Archiv | Byte | ELF-Offset | SHA-256 |
| --- | ---: | ---: | --- |
| `a2tools.json.gz` | 1648 | 2944083 | `536fa0e7b11bb205ee3b8904c801bc516c5b1f3f2dc9963ecd43fe0389847a52` |
| `abysslogs.json.gz` | 2927 | 2945731 | `74cd4656a353009b1ec309a51e8a22fd5d7e6fea5b6de7f95254edd198f9c709` |
| `aiondps.json.gz` | 570 | 2948658 | `6aa8beff10fd4138eda0df7786d400cc82b024bcc7290b06cffc9685828a1154` |
| `jameter.json.gz` | 3292 | 2949228 | `502cca3ce5aed65964ee1a361793a556bff07998d792eb86282001768694acbe` |
| `notmeter.json.gz` | 2032043 | 2952520 | `e698800243594e2d2b28a9336f8c159417cef6a6b41cf71e3f58a3f5b5acc545` |
| `questlog.json.gz` | 701 | 4984563 | `62d5c93923dd67fa592c34a008c144e31fa8d65df2366b9dda820ae14f8b3263` |

`include_bytes!` bettet die kompletten gzip-Bytes einschließlich Quellen- und
Metadaten ein. Beim Start dekodiert `embedded_archives()` alle sechs Dateien,
`load_embedded()` schreibt sie in `provider_archives` und
`provider_observations`. `community_reference_rows` ist davon getrennt.
Die Paketskripte übernehmen dasselbe Binary für tar, deb und rpm. Eine fehlende
lose `data/community`-Datei im installierten Paket verhindert daher nicht die
Weitergabe: Die Daten stecken bereits im Programm. Die gzip-Kompression ist
keine Verschlüsselung.

## Technische Alternative und Funktionswirkung

Diese Änderung entfernt die sechs Dateien, den Einbettungs-/Dekompressionsweg,
den automatischen Startimport und das Vorbereitungsskript für eingebettete
Archive. Auch die direkte flate2-Abhängigkeit entfällt; transitive Abhängigkeiten
anderer Bibliotheken bleiben unberührt. Es gibt keinen Download als Ersatz.

`build-references`, JSON-Import und `--reference-db` bleiben verfügbar. Builder,
SQLite-Leser und DB-Importgrenze verlangen für Anbieterarchive eine ausdrückliche
Rechtebestätigung. Der Leser meldet verworfene Archive; ein fehlender, defekter
oder vollständig unzulässiger Import verhindert den Programmstart nicht.
`rights_confirmed: true` ist weiterhin eine Nutzererklärung, keine automatisch
geprüfte Lizenz. Das Programm kann private Vereinbarungen nicht verifizieren.

Ein frischer Start hat keine Community-Referenzen. Der Community-Bereich meldet
fehlende Vergleichswerte. Lokaler Skill Index, eigene Aufzeichnungen, Aufnahme,
Overlay und Performance Coach bleiben erhalten. Die mathematischen Regeln für
passende, zulässig importierte Vergleichsdaten ändern sich nicht. Aion-DPS-
Gruppenaggregate und Questlog-Indizes waren ohnehin keine absoluten DPS-Mediane.

Alte lokale Archive werden nicht gelöscht, weil automatischer und bewusster
Import bisher nicht getrennt gekennzeichnet wurden. Unbestätigte Archive bleiben
lesbar und werden als inaktiv angezeigt, erzeugen aber keinen Vergleichsscore.
Sie können durch einen ausdrücklich autorisierten Import ersetzt werden.
Bereits berechtigt importierte Referenzen bleiben auch ohne erneuten CLI-Parameter
lokal verfügbar.

Der Paketbau prüft nun Quellbestand und tatsächliches Binary auf Referenzarchive.
Ein Test mit synthetischen gzip-Daten überprüft den Detektor selbst. Zusätzlich
prüfen Rust-/CLI-/UI-Tests Standardstart, autorisierten Import, Rechteablehnung,
fehlerhafte Dateien, alte lokale Daten, erneutes Laden und Bestand eigener Kämpfe.

## Verbleibende Entscheidungen

Diese Änderung bearbeitet weder bestehende Releases/Assets noch Tags oder die
Git-Historie. Sie entfernt die früher verbreiteten Daten nicht aus v0.4.0,
alten Quellarchiven, Klonen oder installierten Datenbanken. Ob die bisherigen
Release-Assets entfernt oder ersetzt werden sollen, ist eine getrennte
Maintainer-Entscheidung. In diesem Auftrag wird keine neue Version veröffentlicht.

Vor einer künftigen gebündelten Auslieferung wäre pro Quelle ein nachvollziehbarer
Beleg erforderlich: Rechtegeber, konkreter Datensatz/Stand, Erlaubnis zur
Normalisierung, Einbettung und Weitergabe über öffentliche GitHub-Quellarchive
sowie Linux-Binärpakete, geltende Lizenzversion, Attribution und etwaige
Bedingungen. Öffentlich lesbare Statistikseiten, eine Code-Lizenz, Quellenangaben
und ein gesetztes Flag ersetzen diesen Beleg nicht. Issue #22 bleibt offen.
