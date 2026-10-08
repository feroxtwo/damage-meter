# Premium Combat Experience — technische Abnahme

Basis: main `468c08d`, Version 0.3.1. Umsetzung auf `design/premium-combat-experience`.
Die drei bestehenden Oberflächen bleiben getrennt: Dashboard `/`, natives eframe/egui-Fenster, Browser-/OBS-Ansicht `/overlay`. Es entsteht kein zusätzliches natives Overlay.

## Informationshierarchie und Signature-Bereiche

- Live: Ziel und HP, eigene Rate/Rang/Anteil, Gruppe und Zeit dominieren eine gemeinsame Fläche. Training steht daneben bzw. darunter. Diagnose, Exporte und Recording bleiben erreichbar.
- Kampfbericht: kurze datenbasierte Zusammenfassung, interaktives stärkstes beobachtetes 5s-Fenster, dazu bei Bedarf gespeicherte Skill-Treffer/Ticks. Der Zeitregler, Gruppen-/Spielervergleich, DPS/Gesamtschaden und Glättung bleiben erhalten.
- Boss-Entwicklung: persönliche Rangposition, Abstand zum Bestwert, Vergleich mit vorherigem Durchschnitt und mittlere absolute Abweichung. Bestehender Verlauf bleibt die zentrale Grafik; globale Session-KPIs sind nachgeordnet.
- Native Kompaktansicht: 312 statt 360 logische Pixel, schmale Klassenbalken, stärkere Rate, eigener Zeilenrahmen, separater Ziel-/Gruppenheader und nur bei bekannten HP ein HP-Balken. Das normale Layout zeigt weiterhin Gesamtsumme und Anteil. OBS verwendet dieselbe Hierarchie.

## Aussagegrenzen

Spitzenfenster werden nur ab fünf vollständig beobachteten Sekunden angezeigt. Kürzungen, Zähler-Rücksprünge, Lücken über drei Beobachtungsraster und Parser-Zahlengrenzen werden berücksichtigt. Ein Peak aus gekürzten Daten gilt ausschließlich für den gespeicherten Ausschnitt. Ein 5s-Fenster kann wegen des Beobachtungsrasters bis zu ein Raster länger sein; die tatsächlichen Grenzen und Dauer bestimmen die Rate.

Die Trefferliste zählt gespeicherte Zeitpunkte in `(Start, Ende]` je Spieler-ID und Skillobjekt. DoT und Multihit sind Treffer/Ticks, keine Casts. Aus Zeitpunkten wird weder Skill-Schaden im Fenster noch eine Rotation abgeleitet. Fehlende Zeitpunkte bedeuten keine bestätigte Inaktivität.

Persönliche Boss-Einordnung benötigt einen ausgewählten Charakter, mindestens drei Versuche derselben Klasse und endliche DPS-Werte. Der Vergleichs-Durchschnitt enthält nur vorherige Versuche. Bestwert und Rang gelten für den gewählten Ausschnitt und keine bestätigten Kills. Gleiche Bestwerte teilen Rang 1. Die mittlere absolute Abweichung ist eine deskriptive Konstanzzahl, keine Signifikanz oder Spielbewertung.

Keine Aussagen über Overheal, Shields, rDPS, Dispel, nicht beobachtete Buffentfernung oder reale Protokollvollständigkeit. Messwerte und Datenbankschema bleiben unverändert. Lokale Icons, IDs, Sprachwahl und anonymisierte Exporte verwenden die bestehenden Pfade.

## Prüfungen und Evidenz

Abnahmestatus wird nach den CI-Läufen und der zweiten Sichtprüfung ergänzt. Lokale reine JS-Prüfungen: Insight-Grenzfälle, vorhandene Diagramm-/PNG-Konsistenz und Skill-/Sprach-/Export-Prüfungen bestanden. Browser- und native Prüfungen sind bis zur CI-Evidenz offen.

Native gefüllte Szenarien verwenden ausschließlich im Debug-Build `A2M_NATIVE_FIXTURE`, maximal 24 synthetische Zeilen. Das vorhandene Fenster und derselbe Painter werden geprüft. Release-Builds enthalten diesen Eingang nicht; Paketparser und HTTP-API werden damit nicht injiziert. Die Produktions-Browserprüfung startet zusätzlich das Release-Binary mit einer frischen temporären Datenbank ohne API-Interception.

Screenshot-Baseline: bereits gespeicherte Dashboard-/Statistik-Aufnahmen von `b7910b1` (in main `468c08d` enthalten; nachfolgende Änderung betrifft Resize-Drosselung). Native Baseline: vorhandene `docs/review-images/actual-native.png` und `actual-native-2x.png`. Neue Aufnahmen werden als CI-Artefakte bereitgestellt, keine erneute Baseline-Testserie.

## Separate reale Ingame-Abnahme — weiterhin offen

1. Reale AION-2-Pakete mit Uhrzeit, Charakter/Klasse, Boss/Schwierigkeit und unabhängiger Referenz sichern.
2. Schaden, Heilung und erlittenen Schaden über Live, Historie, Skilldetails und Replay vergleichen; unbekannte IDs/DoT/Varianten und DE/EN-Namen dokumentieren.
3. Beobachtete 5s-Peaks mit echten Ereignissen und Erfassungslücken abgleichen; Treffer/Ticks nicht als Casts interpretieren.
4. Gleichartige Bossversuche für denselben Charakter/Klasse prüfen, Erfassungsbeginn und Kill-/Wipe-Erkennung separat bestätigen.
5. Das bestehende native Fenster über AION 2 bei hellen/dunklen Szenen, 60–250 %, langen Namen, vielen Zeilen, Tod, Pinning, Lock/Click-through und gespeicherter Position testen.
6. KDE/Wayland, KWin-Regel, Vollbild/Borderless, Proton und Monitor-/DPI-Wechsel auf realem System abnehmen. Xvfb/X11 ersetzt diese Prüfung nicht.
7. OBS mit lokalem Browser-Overlay, Streaming-Anonymisierung und lokalem PNG-Export prüfen.

Technische Freigabe ist keine bestätigte Ingame-Genauigkeit und keine bestätigte Lesbarkeit in weniger als einer Sekunde unter Spielbedingungen.
