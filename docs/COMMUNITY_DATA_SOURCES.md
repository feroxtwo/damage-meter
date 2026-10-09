# Community Skill Index: Datenquellen und Import

## Status und Grenzen

Das Dashboard kombiniert zwei **strikt getrennte** Bewertungsarten:

1. **Lokaler Skill Index:** eigene aufgezeichnete Bosskämpfe gegen den Median anderer lokal beobachteter Spieler im selben KP-Bereich. Kein weltweiter Rang.
2. **Community-Referenzen:** Aggregierte Statistiken eines externen Anbieters, die der Nutzer **mit nachgewiesener Nutzungsberechtigung selbst als JSON** in die lokale Meter-Datenbank importiert. Die Werte werden nicht durch die Anwendung unabhängig verifiziert. Ein aus Referenz-Median errechneter Score ist **kein Perzentil**.

Das Programm fragt keine Community-Seiten automatisiert ab. Es verschickt weder Charakterprofile noch Pakete, Logs oder Kämpfe an einen Drittanbieter. Das ist bewusst so: Für A2 Tools ist der Upload eines konformen Evidence Slice dokumentiert, aber kein autorisierter Statistik-Download. Aion DPS hat öffentliche API-Routen im GitHub-Quellcode, doch der Zugriff und die Wiederverwendung externer Daten sind nicht bestätigt. Die Community-Referenz-Erweiterung ist für eine später genehmigte JSON/API-Zufuhr vorbereitet.

Spielmetadaten (Dungeon-, NPC- und Skillkataloge) sind bereits integriert und bleiben unabhängig von importierten DPS-Statistiken. Die inoffizielle Bibliothek [aion2-api](https://github.com/nuriland/aion2-api) ist eine mögliche Grundlage für **optionale** Charakterabfragen, aber sie ist absichtlich nicht stillschweigend aktiv: Charaktername und Server dürfen nicht ohne ausdrücklichen Nutzerwunsch abgefragt werden. Historische Kampfkraft wird niemals durch einen aktuellen Profilwert ersetzt.

## Quellen

| Quelle | Einsatz | Status |
| --- | --- | --- |
| [A2 Tools Statistiken](https://a2tools.app/stats) | Boss-/Klassenmediane, Stichprobenzahlen, CP/GS-Bänder | Betreiberfreigabe und Datenexport-Format erforderlich |
| [A2 Tools Fremdmeter-Anbindung](https://github.com/taengu/A2Tools-DPS-Meter/blob/main/docs/third-party-meters.md) | Freiwilliger Upload von anonymisierten Evidence Slices nach Konformitätsprüfung | Separater möglicher Integrationsweg, **hier nicht aktiviert** |
| [Aion DPS](https://aiondps.com) und [Quellcode](https://github.com/SkeeveAN/Aion-DPS-Meter) | Bosslisten, Leaderboards, Referenzinformationen | Public-API-Verfügbarkeit und Datenrechte nicht bestätigt |
| [Abyss Logs](https://abysslogs.com/) | Bosslogs, Klassen- und Skillanalyse | Nur Referenz-/URL-Registry; Rechte und Lese-API offen |
| [Questlog Combat Logs](https://questlog.gg/aion-2/en/app) | Kampf- und Performanceberichte | Nur Referenz-/URL-Registry; Rechte und Lese-API offen |
| [JaMeter](https://jameter.net/en) | Community-Kampfrankings | Nur Referenz-/URL-Registry; Rechte und Lese-API offen |
| [NotMeter](https://notmeter.com) | DPS-Vergleiche, teilweise andere DPS-Definition | Nur Referenz-/URL-Registry; Rechte und Lese-API offen |
| [AION 2 API Client](https://github.com/nuriland/aion2-api) | Öffentliche Charakterprofile als möglicher optionaler Datenabgleich | Nicht angebunden, ausdrückliche Zustimmung nötig |
| [Aion2.app](https://aion2.app/de/terms) | Spielmetadaten | Kein automatisierter Import/Scraping ohne Erlaubnis |
| Berechtigter eigener JSON-Datensatz | Aggregierte Referenzwerte | **Import funktioniert** |

Der Code von Community-Projekten kann Open Source sein, ohne dass damit alle zugrundeliegenden Spieler- und Spieldaten zur freien Wiederverwendung lizenziert wären.

## Benutzung

Im Dashboard unter **Statistik → Datenquellen** eine Region wählen und ein JSON auswählen. Die Rechtebestätigung muss bei **jedem Import** aktiv angeklickt werden. Beim Import prüft die Runtime den Datensatz erneut; Browserprüfungen allein reichen nicht. Der Import erfolgt über die lokale API nach SQLite, in einer Transaktion. Bereits vorhandene Snapshots für denselben Anbieter und dieselbe Balance-Periode werden ersetzt, die Kampfaufzeichnungen nicht verändert. Jeder importierte Snapshot kann anschließend direkt in der Datenquellenverwaltung wieder **entfernt** werden.

Die Schaltfläche **JSON-Vorlage** erzeugt absichtlich eine **leere** Vorlage und setzt `rights_confirmed: false`. Ein offizieller Download aus einem externen Portal wird **nicht vorgetäuscht**.

### Format `a2m-community-v2` (neue Vorlage)

Die neue Vorlage deklariert ausdrücklich die Bedeutung ihrer DPS-Zahlen. Nur ein Median über **unabhängige Spieler bei bestätigten Bosskills**, berechnet über die komplette erfasste Kampfzeit (`fight_dps`), darf die strukturierte Vergleichsqualität erhalten. Die Erklärung des Anbieters wird gespeichert, aber **nicht unabhängig verifiziert**.

```json
{
  "schema": "a2m-community-v2",
  "source": {
    "id": "community",
    "url": "https://example.org/permitted-data",
    "captured_at": 1790000000000,
    "rights_confirmed": false
  },
  "balance": {
    "id": "period-example",
    "from_ms": 1780000000000,
    "until_ms": 1792000000000
  },
  "methodology": {
    "metric": "fight_dps",
    "outcome": "confirmed_kill",
    "aggregation": "median_unique_players",
    "patch_id": "replace-with-game-patch"
  },
  "rows": []
}
```

Vor dem Import sind eine echte, erlaubte Datenquelle, überprüfte NPC-/Gebiets-IDs, zusammengehörige regionale Versionen, gültige Werte und eine bewusst abgegebene Rechtebestätigung erforderlich. `rows` bleibt in der Vorlage **leer**; sie enthält keine fiktiven DPS.

### Alte Datensätze: `a2m-community-v1`

Bestehende V1-Importe bleiben lesbar und löschbar. Sie werden **nicht automatisch** als Fight-DPS aus bestätigten Kills uminterpretiert Weil ihre DPS-Methode nicht deklariert ist, ergeben sie **keinen Score**; der Kampfbericht nennt sie als „Daten vorhanden, Vergleich nicht freigegeben“. Die SQLite-Migration ergänzt die neuen Metadaten mit `unknown`, ohne ihre bisherigen Messungen zu verändern.

V2-Werte ergeben nur dann einen Score, wenn die importierte Methode exakt stimmt **und** der lokale Kampf einen erfassten Zieltod sowie keinen als unvollständig markierten Schadensverlauf hat. Abweichende oder unbestätigte Kampfabschlüsse erhalten keinen präzisen V2-Vergleich. Diese Einschränkung kann zu bewusst leeren Ergebnissen führen.


Für jeden tatsächlichen Import müssen alle Beispielwerte durch verifizierte, zur Wiederverwendung freigegebene Aggregatdaten ersetzt werden. `source.id` ist `community`, `a2tools`, `aiondps`, `abysslogs`, `questlog`, `jameter` oder `notmeter`; bei benannten Diensten muss die HTTPS-Quelladresse exakt zur Domain gehören. Eine solche Quellenangabe **beweist keine Authentizität**. `rights_confirmed` muss durch eine ausdrückliche Bestätigung des Nutzers auf `true` gesetzt werden.

`balance.from_ms` und `balance.until_ms` sind Unix-Zeitstempel in **Millisekunden** für die Gültigkeit eines konkreten Balance-Zeitraums (maximal 180 Tage). `captured_at` ist das Datum des zugrunde liegenden Datensatzes. Die Zeilen enthalten den **Median** des Schadens pro Sekunde, nicht Maximalwert, Top-10-Liste, Perzentil oder den Schaden pro Treffer.

Akzeptierte Regionen: `ALL` (bereits über Regionen aggregiert), `EU`, `NAE`, `NAW`, `SA`, `ASIA`, `KR`, `TW`. `class_key` ist einer der neun bekannten Klassenschlüssel oder `all` (bereits über Klassen aggregiert). Die Kampfkraft ist die AION-2-`combat_power` in derselben Einheit wie die lokal aufgezeichnete, **nicht** der Gearscore. KP-Bänder dürfen höchstens 20.000 Punkte breit sein; `samples` muss mindestens 5 sein. Es dürfen höchstens 500 eindeutig definierte Zeilen in einer Datei vorkommen.

## Vergleichsregeln und Datenschutz

Ein Community-Score erscheint nur, wenn **Boss-NPC-ID, Dungeon-ID inklusive Schwierigkeit, genau die gewählte Region, genau die eigene Klasse, Kampfkraftfenster, Balance-Zeitraum, deklarierte Kampf-DPS-Methode und ein bestätigter Kill mit vollständiger Aufzeichnung** passen. Es gibt keine automatische Lockerung: Zeilen für `ALL` (bei gewählter Einzelregion) oder Klasse `all`, Datensätze ohne Methodenangabe und Kämpfe ohne bestätigten Kill werden als „Vergleich nicht freigegeben“ mit Grund gemeldet, aber nie verrechnet. Das entscheidet die Geschäftslogik (`Db::community_index`), nicht die Oberfläche. Anbieterbeobachtungen liegen in eigenen Tabellen und werden dort nur gezählt und mit Grund gemeldet. Training, kurze Kämpfe, unbekannte Kampfkraft und begrenzte Parserwerte bleiben ausgeschlossen. Kein Mischen verschiedener Provider zu einem künstlichen Durchschnitt. Wenn nichts passt, steht bewusst **kein** Community-Score.

Die lokale Quelle wird niemals von einem Import überschrieben. Kein Import erzeugt einen Netzwerkaufruf zu A2 Tools oder Aion DPS. Die gespeicherten Daten bleiben in der lokalen SQLite-Datei des Meters.

## Technische Schnittstellen

- `GET /api/references/sources`: Quelle-Registry und lokal importierte Snapshots.
- `POST /api/references/import`: lokale JSON-Importoperation, geschützt durch den vorhandenen `x-a2m`-Header.
- `DELETE /api/references/{source}/{balance}`: Entfernen eines importierten Snapshots, ebenfalls mit `x-a2m`-Schutz.
- `GET /api/fights/{id}/community-index?region=EU`: Vergleich für einen gespeicherten Bosskampf.
- `GET /api/fights/{id}/skill-index`: unabhängiger, lokaler Vergleich.

**Offen für einen echten automatischen Online-Datenfeed:** schriftlich bestätigte Lese-API oder Betreiberfreigabe, Rate Limits, Antwortschema, Balance-Kohorten, Lizenz, Versionsstrategien, Cache/TTL, Schutz vor übermittelten Spielerdaten und Einwilligungs-UX. Bis dahin **kein Scraping**.


## Eigene Offline-Referenzdatenbank (#22)

Anbieterwerte können einmalig als geprüfte JSON-Snapshots im oben beschriebenen
Format zusammengetragen und zu einer eigenen, verteilbaren SQLite-Datei gebündelt
werden. Der Meter benötigt beim Vergleich keine Verbindung zum Anbieter.

```sh
aion2-meter build-references --output community-references.sqlite a2tools-patch.json abysslogs-patch.json
aion2-meter --reference-db community-references.sqlite --no-overlay
```

Der erste Befehl läuft ohne Spiel, Dashboard oder Capture-Rechte. Er prüft alle
Eingaben vor dem Schreiben und verweigert das Überschreiben vorhandener Dateien.
Die Datenbank enthält ausschließlich aggregierte Snapshots oder normalisierte
Anbieterbeobachtungen mit Herkunft und belegbarer Methodik; keine Charakterprofile oder eigenen Kampfaufzeichnungen. Ein
Anbieter/Balance-Paar darf je Datenbank nur einmal vorkommen. Bis zu 1000 Snapshots
mit jeweils maximal 500 Referenzgruppen sowie maximal sieben Anbieterarchive
mit jeweils maximal 20.000 Beobachtungen sind möglich (32 MiB Gesamtgrenze).

Beim Start liest der Meter die Datei nur lesend, validiert sämtliche Snapshots
und übernimmt sie in seine lokale Referenztabelle. Wiederholte Starts mit diesem
Parameter ersetzen dieselben Anbieter/Balance-Paare; andere importierte Zeiträume
und Kampfaufzeichnungen bleiben erhalten. Entfernte Snapshots werden bei einem
weiteren Start mit demselben Parameter wieder eingelesen. Für eine einmalige
Übernahme den Parameter bei späteren Starts weglassen. Neuere Referenzversionen
werden als neue Datei erstellt und können mit dem gleichen Parameter geladen werden.

Eine eigene Datenbank löst die Laufzeitabhängigkeit vom Anbieter. Sie ersetzt
nicht die Prüfung der Herkunft und Vergleichbarkeit. Insbesondere werden aus
einem Median der Kampfkraft keine KP-Grenzen und aus einer Rangliste keine
Klassenmediane abgeleitet. Es wird kein Median aus mehreren Anbieter-Medianen
gebildet. Die Datenbank übernimmt die Rechtebestätigung aus den Eingabedateien;
sie bestätigt diese nicht selbst. Die Berechtigung muss auch die gewünschte
Weitergabe der gebündelten Daten umfassen.

## Mitgelieferter Anbieter-Datenstand: 9. Oktober 2026

`data/community/community-references.sqlite` enthält **9.272 normalisierte
Datengruppen aus allen sechs aufgeführten Community-Anbietern**. Sie wurde einmalig
für diesen Auftrag aus öffentlichen Seiten bzw. den von diesen Seiten geladenen
öffentlichen Datenantworten erstellt. Der laufende Meter ruft keinen Anbieter ab.
Dies ist ein konkreter Ausschnitt, kein vollständiger Spiegel sämtlicher Anbieter,
Regionen oder historischer Datenstände.

| Anbieter | Datengruppen | Erfasster Umfang |
| --- | ---: | --- |
| A2 Tools | 16 | 8 Klassen in der Standardansicht sowie 8 Klassen für Vakron, EU, Dungeon 600072, Boss 2300812, KP 71000–76000 |
| Aion DPS | 3 | Vakron-Ranglistenaggregate: Runzahl, beste Gruppen-iDPS, mittlere Dauer |
| Abyss Logs | 25 | Vakron-Klassenmediane und Verteilungen nach Anbieter-KP-Bändern, All-Time, Build `global` |
| Questlog | 8 | EU-Klassenübersicht, normalisierter Boss-DPS-Index |
| JaMeter | 87 | Öffentliche KP/DPS-Referenztabellen aller neun Klassen |
| NotMeter | 9.133 | Alle Klassenaggregate der veröffentlichten Kampf-Filteransichten der KR/TW-Cachegeneration `578d3695ce598fe2` |

Die Anbieter-JSON-Dateien unter `data/community/providers/` enthalten Quellen-URLs,
Erfassungszeitpunkte, SHA-256 der Eingabeantworten, Originalfilter und Zahlen. Der
Questlog-Beleg stammt aus der im Browser gerenderten EU-Klassentabelle; sein Hash
bezieht sich auf die daraus erfassten Tabellenwerte, nicht auf einen HTML-Download.
Bei NotMeter wurden Spielerlisten, Namen, Charakterprofile und Rohkämpfe aus der
Antwort ausgeschlossen. Anbieter-Datenstand und Erfassungszeit sind getrennt.
Hashes dokumentieren die Eingabedateien; sie sind keine Anbieter-Signaturen.

### Kompatibilität mit unserem Datenmodell

Das Beobachtungsschema `a2m-provider-observations-v1` verwendet unsere neun
`class_key`-Werte, normalisierte Regionen, numerische DPS und KP sowie optionale
Spiel-IDs. `Brawler` wird `fighter`, `Spiritmaster` wird `elementalist`. Koreanische
`만`-Werte werden mit 10.000 und `k`-Werte mit 1.000 multipliziert. Unterschiedliche
regionale KP-Größen werden nicht durch eine erfundene Skalierung angeglichen.

NotMeter-Dungeon-IDs werden aus den veröffentlichten `mapIds` übernommen, wenn
sie eindeutig sind. Bossnamen werden exakt gegen den koreanischen NPC-Katalog des
gepinnten Parsers (`82e53c1`) abgeglichen; mehrere passende IDs bleiben als
Kandidaten erhalten, ohne willkürlich eine auszuwählen. Exklusive KP-Obergrenzen
werden als inklusive Grenze minus 1 gespeichert. Bei nicht belegter
Grenzkonvention bleibt die Originalangabe samt Unsicherheit erhalten.
Aion-DPS-interne IDs werden ausdrücklich nicht als Spiel-IDs interpretiert.

Jede Zeile enthält `scope`, `statistics` und `compatibility`. Fehlende Angaben
bleiben `null`. Ein Index (Questlog), Gruppen-iDPS (Aion DPS), Training (NotMeter)
und absolute Klassen-DPS bleiben getrennte Kennzahlen. Aus einem Quartil wird kein
Median erzeugt; ein 50.000er-KP-Band wird nicht in erfundene 20.000er-Kohorten zerlegt.

**Aktuell 0 freigegebene Kampf-Score-Kohorten:** Die Werte sind strukturell
vereinheitlicht und lokal abrufbar. Keiner der erfassten Datensätze belegt jedoch
alle für `a2m-community-v2` erforderlichen Vergleichsbedingungen. Unter anderem
fehlen feste Balance-Zeitgrenzen, bestätigte Methodengleichheit oder die genaue
regionale Kohorte. Das Archive enthält daher keine automatisch freigegebenen
`snapshots`; `rights_confirmed` bleibt `false`. Eine Quellenkopie stellt keine
Bestätigung von Weitergaberechten dar. Geprüfte V1/V2-Snapshots können zusätzlich
in derselben SQLite-Datei gebündelt werden.

### Nutzung und Prüfung

```sh
# Die eingebettete Datenbank wird ohne zusätzliche Datei automatisch geladen:
aion2-meter --no-overlay

# Optional eine eigene Datei statt der eingebetteten Datenbank verwenden:
aion2-meter --reference-db /pfad/community-references.sqlite --no-overlay

# Aus den enthaltenen Anbieter-JSON-Dateien reproduzierbar neu bauen:
aion2-meter build-references --output /tmp/community-rebuilt.sqlite data/community/providers/*.json
```

Unter Community-Daten erscheint anschließend die eigene Offline-Anbieterdatenbank
mit Quellennamen, Datengruppenzahl und Erfassungsdatum. `Daten ansehen` öffnet die
lokale JSON-Ansicht. `GET /api/references/archive/{source}?offset=0&limit=50`
liefert die normalisierten Werte paginiert (maximal 200 pro Anfrage).
Der Quellenkatalog unter `/api/references/sources` ergänzt `offline_data`.
Wiederholtes Laden ersetzt die jeweilige Anbieter-Beobachtungssammlung innerhalb
einer Transaktion. Die Score-Referenztabellen und eigenen Kämpfe bleiben getrennt.

In SQLite kann direkt auf die virtuelle Tabelle `observations` zugegriffen werden:

```sql
SELECT source_id, class_key, dungeon_id, mob_code, cp_min, cp_max,
       median_dps, samples, metric, compatibility
FROM observations
WHERE source_id = 'a2tools' AND region = 'EU';
```

`scripts/normalize-community.py` normalisiert gespeicherte Rohantworten ohne
Netzwerkzugriff. Es erwartet die im Skript bezeichneten Eingabedateien und den
koreanischen NPC-Katalog des gepinnten Parsers. Für die gebündelte SQLite-Datei
reichen die mitgelieferten normalisierten JSON-Dateien; Rohantworten und der
152-MB-NotMeter-Gesamtdownload werden nicht mitgeliefert. Der Normalisierungslauf
ist kein automatischer Updatefeed.

Die SQLite-Datei wird beim Build in das Binary eingebettet und schreibgeschützt im Speicher gelesen. Tar-, DEB-, RPM- und Einzelbinary-Installationen besitzen daher dieselben Offline-Daten, auch ohne Repository-Verzeichnis. Beim Start werden die validierten Beobachtungen in die lokale Meter-Datenbank übernommen. `--reference-db` ersetzt die eingebettete Sammlung für diesen Start. NotMeter-Zeiträume erhalten `period_label` und explizite Wochen-Grenzen; Inklusivität bleibt unbestätigt. Eine Normalisierung bestätigt keine Weitergaberechte oder Score-Kompatibilität. Details: [NotMeter-Normalisierung](NOTMETER_NORMALIZATION.md).

### Robustheit, Aktualisierung und API

- Eine fehlende, beschädigte, fremde oder leere `--reference-db` verhindert den Start nicht; sie wird protokolliert und die eingebettete Datenbank verwendet. Jede Quelle wird vollständig geprüft, bevor etwas in die Meter-Datenbank geschrieben wird.
- Ein unveränderter Datenstand wird nicht bei jedem Start neu geschrieben. Ein älterer Erfassungsstand (`captured_at`) ersetzt nie einen neueren derselben Quelle; pro Anbieter ist genau ein Datenstand aktiv, alte und neue Stände werden nicht gemischt.
- Der Builder lehnt Zeilen ab, die sich nur in ihren Zahlen unterscheiden (verlorene Dimension), außerdem Stichprobe 0 und ungültige Kennzeichen. `scripts/normalize-community.py` prüft dasselbe für alle Anbieter vor dem Schreiben.
- Ein Rust-Test baut die SQLite-Datei aus `data/community/providers/*.json` neu und verlangt eine byte-identische Datei; die committete Datenbank kann damit nicht unbemerkt von ihrer Quelle abweichen.
- `GET /api/references/archive/{source}` liefert höchstens 200 Zeilen je Seite, dazu `limit`, `next_offset`, `rights_confirmed` und immer `score_eligible: false`. Ungültige Parameter ergeben 400, unbekannte Anbieter 404.
- `GET /api/fights/{id}/community-index` meldet `withheld` (passende, aber nicht freigegebene Referenzzeilen mit Grund) und `offline_observations` (Anzahl, Anbieter, Gründe). So bleiben „keine Daten“ und „Daten vorhanden, Vergleich nicht freigegeben“ unterscheidbar.
- Alle eingebetteten Anbieterdaten tragen `rights_confirmed: false`. Einbetten heißt, dass jede Installation sie weitergibt; eine Rechtebestätigung ist damit nicht verbunden.
