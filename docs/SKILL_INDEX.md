# Lokaler Skill Index

Der Skill Index ist eine **lokale** experimentelle Leistungskennzahl, kein globales Ranking und kein externer Datenabgleich.

## Berechnung

Für einen gespeicherten Bosskampf: gleiche Dungeon-/Schwierigkeits-ID und Mob-ID, nur Kämpfe ab 10 Sekunden mit validen Zahlen, keine Trainingseinheiten. Die Kampfkraft muss erfasst und positiv sein. Vergleichsspieler müssen im Bereich eigener KP ±10.000 liegen. Unbekannte, maskierte und als eigene Charaktere erkannte Spieler zählen nicht. Ein Spieler mit mehreren Begegnungen wird nur einmal berücksichtigt: zuerst sein Median, anschließend der Median über verschiedene Spieler. Mindestens **5 unabhängige Vergleichsspieler** sind nötig.

`Skill Index = eigene DPS / Median der Vergleichsspieler-DPS × 100`.

Anzeigen: alle Klassen und gleiche Klasse; für die Klassenanzeige müssen mindestens fünf beobachtete Mitspieler derselben bekannten Klasse vorliegen. Daten bleiben vollständig lokal. Der Score bewertet keine Rotation direkt, berücksichtigt keine unterschiedlichen Buffs/Gruppenbedingungen und ist **kein Vergleich mit allen AION-2-Spielern**.

## Anzeige und API

Der Kampfbericht zeigt den Skill Index oberhalb der bisherigen Vergleichsdetails. Read-only `GET /api/fights/{id}/skill-index` gibt Status, KP-Band, Referenz-DPS, Peerzahl und Index zurück; unbekannte Kämpfe erhalten HTTP 404, unzureichende Daten einen erklärten leeren Status. Unabhängig von einem fehlenden Score bleiben DPS und Kampfbericht sichtbar.

## Verifikation

Der Unit-Test `skill_index_medians_are_per_distinct_peer_and_scope_is_strict` prüft Score 130 aus fünf Spielern, wiederholte Kämpfe desselben Spielers, falsche Gegner, falsche KP, Training, Zahlenbegrenzung, zu kurze Kämpfe, geringe Datenmenge, fehlende KP und unbekannte Kampf-ID. Zusätzliche CI-Prüfung des Gesamtprojekts erforderlich. Es wurden hier keine lokalen Tests ausgeführt.
