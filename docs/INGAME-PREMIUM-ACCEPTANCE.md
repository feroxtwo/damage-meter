# Reale AION-2-Ingame-Abnahme

Status: **offen**, Stand 8. Oktober 2026. Diese Liste ergänzt die [bisherige technische Abnahmematrix](PREMIUM-UX-ACCEPTANCE.md) und den [Combat-Telemetry-Abschluss](COMBAT-TELEMETRY-ACCEPTANCE.md). Synthetische Browser-, X11- und Replay-Tests bestätigen keine reale Protokollgenauigkeit und keine Lesbarkeit während eines echten Kampfes.

Für jeden Durchlauf festhalten: Meter-Commit, Spielversion, Uhrzeit, Charakter/Klasse, Ziel/Schwierigkeit, Sessiontyp X11/Wayland, Desktop/Compositor, Monitorauflösung/DPI, Proton und Vollbildmodus. Testdaten vor Weitergabe anonymisieren; die unabhängige Referenz und Abweichungen nachvollziehbar aufbewahren.

| Offen | Prüfung | Erforderlicher Nachweis |
|---|---|---|
| ☐ | Reale Pakete und unabhängig beobachtete Kampfereignisse sichern | Zeitlich zuordenbare Referenz; Erfassungsbeginn, Unterbrechungen und fehlende Pakete dokumentiert |
| ☐ | Schaden, Heilung und erlittenen Schaden über Live, Skilldetails, Historie, PNG und Replay vergleichen | Summen, Zeitfenster und Raten mit Referenz abgleichen; Abweichungen mit stabilen IDs aufführen |
| ☐ | Fähigkeiten, Varianten und DoT auf Deutsch und Englisch prüfen | Namen/Icon-Zuordnung anhand IDs; unbekannte IDs bleiben verständlich, Community-Namen eindeutig markiert |
| ☐ | Lokales Burst-Signal mit Live-Beobachtungen vergleichen | Nur empfangene eigene Werte, getrennte Charaktere/Ziele, sichtbare Polling-Lücken und korrekt unbekannte HP; kein rekonstruierter Altverlauf |
| ☐ | Gespeicherten Versuch nach Zielwechsel prüfen | Bericht passt über ID, Start, Boss und eigenen Charakter; fehlender Datensatz erzeugt keine Ergebnisbehauptung, kein automatischer Siegtext |
| ☐ | Training und Bestwertzustände im Spiel prüfen | Erster Treffer, reale Trainingszeit, eigene Schadenssumme, Charakter/Ziel/Dauer und bestätigter Bestwert; Abbruch bleibt ohne fertiges Ergebnis |
| ☐ | Persönlichen und Gruppen-Peak nachprüfen | Wirklich beobachtetes 5s-Fenster samt Rastergrenzen; Kürzungen und Lücken korrekt gekennzeichnet, Treffer/Ticks nicht als Casts interpretiert |
| ☐ | Boss-Entwicklung am selben Charakter und derselben Klasse prüfen | Rang/Bestwert im gewählten Ausschnitt und Vergleich mit vorherigem Durchschnitt nachvollziehbar; Kill-/Wipe-Erkennung separat prüfen |
| ☐ | Bestehendes natives Overlay während heller und dunkler Kämpfe lesen | Ziel, Zeit, eigene Rate/Rang und Gruppenrate schnell erfassbar; menschliche Prüfung des Ein-Sekunden-Ziels |
| ☐ | Native Skalierung und Zeilenstress prüfen | 60/100/150/200/250 %, lange Namen, große Zahlen, viele Spieler, alle Themes, normale/kompakte Zeilen und Tot-Status ohne relevante Überdeckung |
| ☐ | Native Bedienung und Lebenszyklus prüfen | Lock/Click-through, eigene Zeile/Pinning, Sichtbarkeit, gespeicherte Position, Neustart und Zurückholen im echten Spiel |
| ☐ | Reale KDE-/Wayland-/KWin-Umgebung prüfen | Always-on-top, Transparenz, Vollbild/Borderless, Monitor-/DPI-Wechsel und Proton; Desktop/Version explizit angegeben |
| ☐ | OBS und Privacy im Spiel prüfen | Separate Browserquelle mit lokalen Assets, Maskierung von Namen in Anzeige/Tooltip/Export und PNG-Bericht ohne Netzwerkbedarf |
| ☐ | Lange Sitzung und große reale Historie prüfen | Speicher/CPU und Live-Reaktionszeit beobachtet; begrenzte Verlaufs-/Effektdaten und Kürzungshinweise korrekt |

Ohne unabhängig belastbare Referenz bleiben Overheal, Shields, rDPS, Dispel und nicht beobachtete Buffentfernung außerhalb einer bestätigten Aussage. Keine dieser Größen aus fehlenden Ereignissen ableiten.

Abschluss je Punkt mit Datum, Prüfer, Ergebnis und Evidenzlink dokumentieren. Bekannte Einschränkungen nicht als bestanden markieren; reproduzierbare Abweichungen als konkrete Fehler mit Datenausschnitt erfassen.
