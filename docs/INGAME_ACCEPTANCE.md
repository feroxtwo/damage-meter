# Reale AION-2-Abnahme des Releasekandidaten 0.3.1

**Alle Punkte sind offen.** Softwaretests und synthetische Daten ersetzen diese Abnahme nicht. Benötigt werden aktuelle Spielversion, reale Aufnahmen, exakte Referenzwerte derselben Messspanne und die tatsächlich eingesetzte Linux-Desktopumgebung.

## Messprotokoll je Sitzung

- Commit, Meter-/Parser-/Spielversion, Klasse, Charakter, Region, Server, Dungeon und Schwierigkeit dokumentieren.
- Distribution, Kernel, KDE-Version, Wayland/X11, Proton-Version, Grafiktreiber, Monitorauflösungen und Skalierung notieren.
- Meter vor Login/Kampf starten; Berechtigung, Prozess, Verbindung, Paketanzahl und Lücken kontrollieren. Aufnahme vor Kampf aktivieren.
- Definierte Messspanne und ersten/letzten Treffer separat festhalten. Kein Reset innerhalb einer Referenzspanne. Target-ID/Actor-ID im Replay bestimmen.
- Referenz möglichst spielinterne Analyse mit **exakten** vollständigen Skill-Summen, Treffern und Zeitspanne; gerundete Screenshots reichen nicht für exakte Gleichheit. Zusätzlich unabhängigen Meter aufnehmen, dessen abweichendes Zeitmodell dokumentieren.
- Originalaufnahme lokal sichern. Rohaufnahmen enthalten Identitäten und Verbindungsdaten; nicht ungeprüft veröffentlichen. Anonymisierte JSON/CSV/PNG-Exporte getrennt prüfen.

## Abnahmematrix

| Testfall | Ablauf / Sollnachweis | Status |
|---|---|---|
| Gesamtschaden / Spieleranteile | Einzelziel ohne Overkill; vollständige Skill-Summen pro Spieler und Gruppe gegen exakte unabhängige Referenz | Offen |
| DPS / Zeitfenster | Ersten und letzten erfassten Zieltreffer bestimmen; Summe / max(Dauer, 1 s); Abweichung zu aktivzeitbasierten Referenzen erklären | Offen |
| HPS / Heilung | Einzelne direkte Heilung, HoT, Gruppenheilung und Overheal getrennt; reine Ressourcenwiederherstellung nicht als Heilung | Offen |
| Kampfstart / Kampfende / Dauer | Unmittelbarer Start, längere Pause, letzter Treffer, Ziel-Tod und Reset; Pause nach letztem Treffer verlängert unsere DPS-Dauer nicht | Offen |
| Skilldetails | IDs/Namen, Treffer/Ticks, Min/Max/Mittelwert, Anteil und Skill-DPS; mehrere Skills und DoT-Varianten | Offen |
| Crits / weitere Merkmale | Kontrollierte Crit-, Rücken-, Frontal-, Double-, Multihit-, Block-, Parry- und Resist-Fälle; unbekannte Merkmale bleiben unbekannt | Offen |
| Spieler / Party | Solo, Vierergruppe, große Gruppe, spät beigetretene/ausgetretene Spieler, identische Namen, mehrere Server | Offen |
| Pets / Summons | Bekannte Besitzer, zwei gleiche Klassen, spätes Spawn-/Besitzerpaket, Beschwörung ohne Namen, Ground-Effekte | Offen |
| DoTs / HoTs | Mehrere Ticks, Refresh, Multihit, letzter Tick nach Tod/Wechsel, gleiche Skills auf mehreren Zielen | Offen |
| Reflektiert / Overkill | Wenn Protokoll unterstützt: Rückwurfzuordnung und Treffer über restliche HP; gegen Referenz abgleichen, unbekannte Behandlung dokumentieren | Offen |
| Tod / Wiederbelebung | Einzelner Tod, Respawn, neue Treffer nach Wiederbelebung; Death-Marker nicht in neuen Kampf übernehmen | Offen |
| Buffs / Debuffs | Anwendung, Refresh, mehrere Caster, frühes Entfernen/Dispel; bekannte Entfernungslücke ausdrücklich messen | Offen |
| Instanzwechsel | Eintritt, Wechsel Schwierigkeit, unmittelbar nach Boss verlassen, Partyauflösung; kurze Versuche gespeichert und Run-Zuordnung korrekt | Offen |
| Expedition neu starten | Expedition abschließen (mindestens ein Boss getötet), dann im Spiel neu starten: Meldung „Neustart erkannt“, neuer Run unter **Runs**, Bosse des alten Durchgangs bleiben im alten Run. Neustart nach Wipe ohne Bosskill wird nicht automatisch erkannt: **Neuer Run** bzw. `aion2-meter ctl new-run` prüfen. Aufnahme mitschneiden | Offen |
| Open World / Gruppe | Solo neben fremden Spielern am selben Mob: nur eigene Zeile. Mit Gruppe: Gruppenmitglieder sichtbar, Fremde nicht. Einstellung „Open World: andere Spieler anzeigen“ zeigt alle. Im Dungeon immer ganze Gruppe | Offen |
| Disconnect / Reconnect | Ruhige und laufende Verbindung, neue IP/Port, VPN; neuer Stream ohne Doppelschaden, Lücken sichtbar | Offen |
| Charakter-/Klassenwechsel | Mehrere Charaktere derselben Sitzung; Filter und Bestwerte nicht vermischen; Profile/Streaming unverändert | Offen |
| Mehrere Kämpfe | Direkt aufeinanderfolgende Bosse, mehrere Ziele/Adds, Zielwechsel, fehlender Spawn, gleicher Boss nach Wipe | Offen |
| Sehr kurzer Kampf | Ein Treffer, unter eine Sekunde, zwei Treffer; alle Ansichten nutzen unseren mindestens 1-s-Nenner | Offen |
| Sehr langer Kampf | Mindestens 2 h plus lange Trainingssession; Speichergrenzen, gekürzte Kurven, Parser-Zahlengrenzen und UI-Warnung beobachten | Offen |
| Hohe Eventrate / große Gruppe | Burst mit vielen DoTs/Summons, reale Paketverluste und Queue-Rückstau; CPU/RAM/UI/FPS/Stream-Lücken messen | Offen |
| Reset | Manuell mitten/nach Kampf; optional Idle/Wipe; Training ausgenommen; Speicherung vor Clear; Speicherfehler sichtbar | Offen |
| Ende / Neustart | Menü Beenden, Fensterschließen, SIGTERM, Ctrl-C; aktuelle kurze Versuche/Run/Profiles; SIGKILL-Verlustspanne dokumentieren | Offen |
| Overlay im Spiel | Lesbarkeit in hellen/dunklen Szenen, voller Gruppe, Klicks/Fokus, Rechtsklick und Hotkeys während Kampf | Offen |
| KDE / Wayland / X11 | Always-on-top, Click-through, Drag, Borderless, Alt-Tab, randlos/vollbild; KWin-Regel und CLI-Control | Offen |
| Mehrere Monitore / DPI | Negative Koordinaten, gemischte 100/150/200 %, Monitor abstecken/wechseln, Position und Scale nach Neustart | Offen |
| Export / Vergleich | Live, Detail, Historie, Replay und JSON/CSV/PNG derselben Messspanne vergleichen; keine Namen/Notizen/IPs bei anonymem Export | Offen |
| Pakete / Upgrade / Entfernen | Fedora RPM, Debian DEB, Bazzite rpm-ostree und Archiv real installieren/aktualisieren; setcap nach Update; Nutzerdaten bleiben erhalten | Offen |

## Offline-Verifikation der realen Aufnahme

```sh
aion2-meter replay echte-aufnahme.a2mcap --output analyse.json
python3 scripts/validate-combat.py analyse.json referenz.csv --actor ACTOR_ID --target TARGET_ID --tolerance-percent 0.5 --output abnahme.json
```

CSV-Spalten: `kind,code,is_dot,total`, eine Zeile pro Schadens-/Heilungsskill und DoT-Variante. Alle Referenzskills aufnehmen. Toleranz vor Messung festlegen, Abweichungen je Skill prüfen, nicht durch gleiche Gesamtsumme kompensieren. Das Skript prüft Summen, **nicht** das DPS-Zeitmodell oder eine lückenlose Protokollabdeckung.

## Entscheidung

Eine Ingame-Freigabe setzt vollständige unabhängige Referenz, nachvollziehbare Zeitfenster und die betroffene Desktop-/Paketumgebung voraus. Für jede Abweichung Bugreport mit anonymisiertem Export, lokal verfügbarer Aufnahme, Versionen und Reproduktionsschritten erstellen. Ergebnis pro Zeile: bestanden / nicht bestanden / nicht anwendbar mit Begründung. Erst danach im Releasebericht den Status ändern.

## Zusätzliche reale Abnahme: DE/EN-Katalog und Symbole

Diese Erweiterung verändert nur die Anzeige. Für eine Freigabe anhand echter Kampfdaten separat dokumentieren:

| Fall | Nachweis | Status |
|---|---|---|
| Jede verfügbare Klasse | Skill-ID aus realer Aufnahme mit Spielname und Spielsymbol vergleichen, auch neue Faustkämpfer-Fähigkeiten | Offen |
| Varianten / Kombofolgen | Haupt-, Folge-, Spezial-, DoT-/HoT- und Multihit-ID vergleichen; gleicher Name darf keine falsche Klassen-/Symbolzuordnung erzeugen | Offen |
| Gemeinsame Namen | „Defiance“, „Impact Hit“, „Survival Willpower“ in unterschiedlichen Klassen getrennt kontrollieren | Offen |
| Pets / Beschwörungen / Effekte | Paket-ID, Besitzer und Anzeige getrennt prüfen; unbekannte Effekte bleiben erkennbar | Offen |
| Deutsch / Englisch | Dieselbe Messspanne in DE und EN öffnen; primäre ID, Treffer, Schaden, DPS/HPS, Zeiten und Reihenfolge nach numerischer Sortierung identisch | Offen |
| Faustkämpfer DE | Community-Namen mit später verfügbaren offiziellen deutschen Spielnamen abgleichen und nötige Korrekturen dokumentieren | Offen |
| Historie / Replay | Alten Bericht in beiden Sprachen öffnen und reale Aufnahme nachspielen; Zahlen bleiben identisch | Offen |
| Export | JSON und PNG gegen DE/EN-Bericht vergleichen; richtige Skill-/Klassenicons; Anonymisierung von Spielern unverändert | Offen |
| Offline / Neustart | Externe Webseiten unerreichbar, Meter neu starten; Katalog, Icons und gespeicherte Sprache funktionieren weiter | Offen |
| Natives / OBS-Overlay | Alle Klassen, kompakte/normal große Zeilen, 60–200 %, lange Namen und verschiedene Hintergründe; Icons schneiden Zahlen/Namen nicht ab | Offen |
| Unbekannte / neue ID | Rohcode und verfügbarer Name sichtbar, keine falsche Zuordnung zu einer zufälligen Fähigkeit | Offen |

Pro Befund Original-ID, Klasse, Skillname im Spiel, Sprache/Clientpatch, erwartet/angezeigt und zugehörige lokale Aufnahme notieren. Es gibt keine Behauptung, dass ein sichtbares Icon die Genauigkeit des Parsers bestätigt.
