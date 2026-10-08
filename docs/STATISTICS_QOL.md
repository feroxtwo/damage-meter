# Verständlichere Statistik und Schadensverläufe

## Bedienung

Im Kampfbericht steht ein gemeinsamer **Schadensverlauf** vor den Detailvergleichen. **Ansicht** wählt die gesamte Gruppe, einzelne Spieler oder den Spieler-Vergleich. **Wert** schaltet zwischen DPS und gespeichertem Gesamtschaden um. Die DPS sind standardmäßig über fünf Sekunden geglättet; einzelne Beobachtungsintervalle bleiben auswählbar. Die Glättung ist bei Gesamtschaden deaktiviert.

Im Vergleich lassen sich Spieler über die beschriftete Legende aus- und einblenden. Achsen haben Einheiten, Zeitmarken und ein Raster. Die Werte des gewählten Zeitpunkts stehen dauerhaft darunter. Maus und Touch wählen einen Punkt im Diagramm; der Zeitregler ist auch per Pfeiltasten, Home und End bedienbar. Die Punktwahl verwendet sämtliche gespeicherten Beobachtungen, selbst wenn die Linien für lange Kämpfe auf höchstens 601 Punkte reduziert werden. Ping liegt in einem aufklappbaren Abschnitt.

Unter **Statistik** steht der eigene Boss-Verlauf zuerst. Boss und Schwierigkeit sowie letzte 20, letzte 50 oder alle Versuche sind auswählbar. Kennzahlen zeigen letzten Versuch, Bestwert, arithmetischen Durchschnitt und Anzahl im gewählten Ausschnitt. Die gestrichelte Linie markiert diesen Durchschnitt. Versuchsauswahl, Datum, Kampfdauer und Veränderung gegenüber dem unmittelbar vorherigen Versuch erleichtern den Vergleich. **Kampfbericht öffnen** führt zum gespeicherten Bericht, sofern dessen ID vorliegt. Die Bossauswahl bleibt beim Neuladen anhand von Boss und Dungeon-ID erhalten, auch wenn sich die Reihenfolge der Liste ändert.

Die Aktivität der letzten 30 Tage nennt zusätzlich Runs und aktive Tage. Lange Charakter-/Spielernamen und mobile Diagramme werden innerhalb der vorhandenen Karten und Dialoge umgebrochen.

## Bedeutung der Werte

- Kurven stammen aus kumulativen Schadensbeobachtungen, normalerweise im 500-ms-Raster. Sie bilden keine einzelnen Trefferpakete ab. Das gleitende Fenster wird auf gespeicherte Beobachtungen gerundet; am Anfang oder nach einer Kürzung zählt nur die beobachtete Zeit. Es ist eine Darstellungsfunktion und verändert weder Messwerte noch Speicherung.
- Bei einem gekürzten Verlauf dient der erste gespeicherte Gesamtwert ausschließlich als Ausgangswert. Er wird nicht in einen vermeintlichen DPS-Spike umgewandelt. Unvollständigkeit bleibt sichtbar. Gesamtschaden kann bereits vor Beginn des verbleibenden Ausschnitts entstanden sein.
- Alte Berichte ohne Verlaufsdaten erhalten einen erklärenden Leerzustand und einen deaktivierten Zeitregler. Ein einzelner verwertbarer Punkt wird als Punkt angezeigt.
- Bossversuche haben gleiche Abstände in zeitlicher Reihenfolge; die X-Achse benennt ausdrücklich Versuche. Ein Versuch ist kein Kill-Nachweis. Rote Punkte markieren den erfassten eigenen Tod. Fehlende Anteilswerte werden als **nicht erfasst** ausgewiesen.
- Ohne Charakterfilter können verschiedene eigene Charaktere in einer Bossserie stehen. Die Ansicht weist darauf hin; gezielte Vergleiche brauchen die Charakterwahl oben.
- PNG-Berichte behalten ihre vorhandene separate Darstellung und Exportauswahl. Anonymisierung, lokale Symbole und unveränderte Rohwerte bleiben Teil der Export-Regression.

## Technische Abnahme

`node scripts/test-chart-ui.cjs` prüft acht Bereiche ohne Browser: Gruppen-/Spieler-DPS, Gesamtschaden, gleitendes Fenster, gekürzte Ausgangswerte, fehlende/einzelne Samples, sichere Beschriftung, begrenzte Linien und unveränderte Datensätze. `node scripts/test-skill-ui.cjs` prüft weiterhin acht Katalog-/Exportbereiche.

Die echte Chromium-Suite ergänzt vier Fälle: Umfang/Glättung/Gesamtschaden samt Tastatur und Legende; teilweise/fehlende/einzelne Verläufe; dichte Mehrspieler-Kurven auf Desktop und 320-Pixel-Displays; Bosskennzahlen/Versuchsauswahl/stabile Boss-ID und Berichtssprung. Die vorhandenen Export-, Sprach-, Overlay- und Langzeittests bleiben enthalten. Insgesamt sind 45 Browserfälle vorgesehen. GitHub Actions stellt synthetische Screenshots als `web-screenshots` bereit.

Lokal fehlt das Chromium-Binary; diese Einschränkung zählt nicht als bestandene Browserprüfung. Der verbindliche Browserstatus und die übrigen Build-Gates stehen am PR und im zugehörigen CI-Lauf. Eine technische Abnahme ersetzt weiterhin nicht die [Ingame-Abnahme mit echten Kampfdaten](INGAME_ACCEPTANCE.md).

## Geänderte Dateien dieser Ergänzung

| Datei | Änderung |
|---|---|
| `web/index.html` | Bossansicht, Kennzahlen, stabile Auswahl, Versuchregler, Berichtssprung, Aktivitätszusammenfassung |
| `web/enhancements.js` | Gemeinsame Chart-Darstellung, Intervall-/Fenster-DPS, Gesamtschaden, Umfangsauswahl, Legende, Zeitauswahl, Leerzustände |
| `web/qol.js` | Zweites Gruppendiagramm entfernt; bestehender Paarvergleich und PNG-Export bleiben erhalten |
| `web/enhancements.css` | Responsive Diagramme, Bedienelemente, dauerhaft sichtbare Werte und Kennzahlen |
| `scripts/test-chart-ui.cjs` | Berechnungs- und Darstellungsinvarianten ohne Chromium |
| `scripts/test-web.cjs` | Vier neue Browserfälle und Anpassung des bisherigen Gruppendiagramm-Checks |
| `package.json` | Chartprüfung in den Testlauf aufgenommen |
| `.github/workflows/ci.yml` | Synthetische Browser-Screenshots als CI-Artefakt |
| `README.md`, dieses Dokument | Bedienung, Datenbedeutung und technische Abnahme |
