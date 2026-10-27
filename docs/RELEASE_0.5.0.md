# AION2 Meter 0.5.0

## Neue Kampfanalyse

- Echte Skill-Nutzungen mit Rotation, Opener, Nutzungen pro Minute und gemeinsamer Gruppenzeitachse.
- Genauere Buff-Auswertung mit frühen Endereignissen und Beiträgen einzelner Wirker.
- Bossfähigkeiten, eingehender Schaden, defensive Ergebnisse und letzte beobachtete Treffer vor einem Tod.
- Plausibilitätsvergleich zwischen Schaden und beobachtetem Boss-HP-Verlust.
- Räumliches Replay mit Wiedergabe, Zeitregler, Zoom und gemessenen Positionsspuren.
- Performance Coach ergänzt beobachtete Unterschiede bei Skill-Nutzungen.

## Overlay, OBS und Diagnose

- Optionaler Boss-Cast und bis zu acht durchsuchbare Cooldown-Skills im bestehenden Overlay.
- Hoher Kontrast und berechnete Kontrastanzeige.
- Getrennte Diagnose für Paketfamilien und mögliche Parser-Probleme.
- Geschützte, standardmäßig deaktivierte OBS-Freigabe auf eigenem Port. Namen sind zunächst ausgeblendet. Für zwei PCs mit `--stream-listen 0.0.0.0` starten und den Link in Einstellungen kopieren.
- 64-Bit-Summen für neue Schadens- und Heilungsmessungen.
- Einstellungsabfragen werden einmal geladen und gegen veraltete Antworten geschützt. Der zugehörige Web-Testfehler ist behoben.

## Vergleichsdaten

Externe Anbieterarchive werden nicht mehr in das Programm eingebettet oder automatisch importiert. Eigene zulässige Referenzdaten können weiterhin ausdrücklich importiert werden. Vorhandene unbestätigte Archive bleiben lokal erhalten, liefern aber keine Vergleichsscores. Eigene Kämpfe und der lokale Skill Index bleiben erhalten.

## Installation und Update

- Fedora/Bazzite/Kinoite: RPM.
- Debian/Ubuntu: DEB.
- Andere Linux-Systeme: TAR.GZ mit Installationsskript.

Bestehende Einstellungen und Kampfhistorie bleiben beim Update erhalten. Die Pakete werden zusammen mit `SHA256SUMS` veröffentlicht.

## Messgrenzen

Reale AION-2-Messgenauigkeit und KDE/Wayland-Verhalten benötigen weiterhin Ingame-Abnahme. Ältere Kämpfe erhalten keine nachträglich erfundenen Ereignisse. Replay-Positionen sind sporadische Servermessungen. Cooldown-Sekunden mit einem selbst gewählten Zeitfaktor bleiben ausdrücklich geschätzt. Erkannte Cast-Signale bestätigen keinen erfolgreichen Abschluss einer Fähigkeit.

Integriert: Pull Requests #33, #34 und #35.
