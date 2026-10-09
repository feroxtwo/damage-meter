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
| [AION 2 API Client](https://github.com/nuriland/aion2-api) | Öffentliche Charakterprofile als möglicher optionaler Datenabgleich | Nicht angebunden, ausdrückliche Zustimmung nötig |
| [Aion2.app](https://aion2.app/de/terms) | Spielmetadaten | Kein automatisierter Import/Scraping ohne Erlaubnis |
| Berechtigter eigener JSON-Datensatz | Aggregierte Referenzwerte | **Import funktioniert** |

Der Code von Community-Projekten kann Open Source sein, ohne dass damit alle zugrundeliegenden Spieler- und Spieldaten zur freien Wiederverwendung lizenziert wären.

## Benutzung

Im Dashboard unter **Statistik → Datenquellen** eine Region wählen und ein JSON auswählen. Die Rechtebestätigung muss bei **jedem Import** aktiv angeklickt werden. Beim Import prüft die Runtime den Datensatz erneut; Browserprüfungen allein reichen nicht. Der Import erfolgt über die lokale API nach SQLite, in einer Transaktion. Bereits vorhandene Snapshots für denselben Anbieter und dieselbe Balance-Periode werden ersetzt, die Kampfaufzeichnungen nicht verändert.

Die Schaltfläche **JSON-Vorlage** erzeugt absichtlich eine **leere** Vorlage und setzt `rights_confirmed: false`. Ein offizieller Download aus einem externen Portal wird **nicht vorgetäuscht**.

### Format `a2m-community-v1`

Folgendes ist ein **rein fiktives Strukturbeispiel** und darf keinesfalls als reale AION-2-Messung importiert oder verteilt werden:

```json
{
  "schema": "a2m-community-v1",
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
  "rows": [
    {
      "region": "EU",
      "dungeon_id": 600093,
      "mob_code": 2300409,
      "class_key": "gladiator",
      "cp_min": 60000,
      "cp_max": 80000,
      "median_dps": 14500,
      "samples": 120
    }
  ]
}
```

Für jeden tatsächlichen Import müssen alle Beispielwerte durch verifizierte, zur Wiederverwendung freigegebene Aggregatdaten ersetzt werden. `source.id` ist `community`, `a2tools` oder `aiondps`; bei benannten Diensten muss die HTTPS-Quelladresse exakt zur Domain gehören. Eine solche Quellenangabe **beweist keine Authentizität**. `rights_confirmed` muss durch eine ausdrückliche Bestätigung des Nutzers auf `true` gesetzt werden.

`balance.from_ms` und `balance.until_ms` sind Unix-Zeitstempel in **Millisekunden** für die Gültigkeit eines konkreten Balance-Zeitraums (maximal 180 Tage). `captured_at` ist das Datum des zugrunde liegenden Datensatzes. Die Zeilen enthalten den **Median** des Schadens pro Sekunde, nicht Maximalwert, Top-10-Liste, Perzentil oder den Schaden pro Treffer.

Akzeptierte Regionen: `ALL` (bereits über Regionen aggregiert), `EU`, `NAE`, `NAW`, `SA`, `ASIA`, `KR`, `TW`. `class_key` ist einer der neun bekannten Klassenschlüssel oder `all` (bereits über Klassen aggregiert). Die Kampfkraft ist die AION-2-`combat_power` in derselben Einheit wie die lokal aufgezeichnete, **nicht** der Gearscore. KP-Bänder dürfen höchstens 20.000 Punkte breit sein; `samples` muss mindestens 5 sein. Es dürfen höchstens 500 eindeutig definierte Zeilen in einer Datei vorkommen.

## Vergleichsregeln und Datenschutz

Ein Community-Score erscheint nur, wenn **Boss-NPC-ID, Dungeon-ID inklusive Schwierigkeit, Region (mit explizit ausgewähltem `ALL`-Fallback), Klasse oder `all`, Kampfkraftfenster und Balance-Zeitraum** passen. Training, kurze Kämpfe, unbekannte Kampfkraft und begrenzte Parserwerte bleiben ausgeschlossen. Kein Mischen verschiedener Provider zu einem künstlichen Durchschnitt. Wenn nichts passt, steht bewusst **kein** Community-Score.

Die lokale Quelle wird niemals von einem Import überschrieben. Kein Import erzeugt einen Netzwerkaufruf zu A2 Tools oder Aion DPS. Die gespeicherten Daten bleiben in der lokalen SQLite-Datei des Meters.

## Technische Schnittstellen

- `GET /api/references/sources`: Quelle-Registry und lokal importierte Snapshots.
- `POST /api/references/import`: lokale JSON-Importoperation, geschützt durch den vorhandenen `x-a2m`-Header.
- `GET /api/fights/{id}/community-index?region=EU`: Vergleich für einen gespeicherten Bosskampf.
- `GET /api/fights/{id}/skill-index`: unabhängiger, lokaler Vergleich.

**Offen für einen echten automatischen Online-Datenfeed:** schriftlich bestätigte Lese-API oder Betreiberfreigabe, Rate Limits, Antwortschema, Balance-Kohorten, Lizenz, Versionsstrategien, Cache/TTL, Schutz vor übermittelten Spielerdaten und Einwilligungs-UX. Bis dahin **kein Scraping**.
