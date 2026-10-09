# NotMeter-Zeiträume verlustfrei normalisieren

NotMeter verwendet `period: All` sowohl für den gesamten Zeitraum als auch
für Wochenkohorten. Die Wochen stehen im Feld `periodLabel` als
`weekly-wed05|<Start mit Zeitzone>|<Ende mit Zeitzone>`. Ohne dieses Feld
kollidieren unterschiedliche Kohorten mit verschiedenen Messwerten.

Der Offline-Normalisierer erhält `scope.period` und `scope.period_label`.
Für Wochen speichert er zusätzlich `period_kind`, `period_from_ms` und
`period_until_ms`. Die UTC-Millisekunden werden ausschließlich aus den
angegebenen Zeitpunkten berechnet. Die Inklusivität der Grenzen bleibt
unbestätigt. Relative Labels wie „Heute“ erhalten keine erfundenen Zeitgrenzen.

Die Kohortenidentität umfasst Klasse und Quellenscope einschließlich Zeitraum,
Boss, Dungeon, KP-Stufe und Erzeugungszeit. Datensatz- und Spieleranzahl sind
zusätzliche Metadaten, keine Identitätsmerkmale. Mehrfache identische Kohorten
werden abgelehnt, auch wenn ihre Statistikwerte oder Anzahlen abweichen.
Mindestgruppengröße, Quellschema und Quellversion bleiben erhalten.

## Lokal aus gespeicherten Rohdaten erzeugen

```sh
python3 scripts/normalize-community.py \
  --input-dir /privater/pfad/rohdaten \
  --output-dir /privater/pfad/normalisiert \
  --npc-catalog /pfad/zum/gepinnten/parser/src/data/i18n/npcs/ko.json
python3 scripts/test-normalize-community.py
```

Das Eingabeverzeichnis enthält die im Skript benannten gespeicherten Antworten
aller sechs Anbieter. Es gibt keine Netzwerkaufrufe. Die Ausgabe enthält nur
Klassenaggregate; Spieler-Ranglisten aus dem NotMeter-Gesamtobjekt werden nicht
übernommen. Rechte, Region, Patch und Methodenkompatibilität werden durch die
Normalisierung nicht bestätigt. Beobachtungen werden nicht automatisch zu
Score-Referenzen. Diese Änderung veröffentlicht keine Anbieterdatensätze.

Die Regressionstests verwenden ausschließlich synthetische Daten und laufen
im Workflow „Provider normalization“.
