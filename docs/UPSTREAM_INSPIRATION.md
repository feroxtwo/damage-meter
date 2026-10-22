# Combat insights: Herkunft, Umfang und Messgrenzen

Basis: main 5517139e8b777749aadf39f6671d246a1c2defc7.

## Referenzen

- SkeeveAN/Aion-DPS-Meter, gesichteter Tree fafba135eb4e5e35e09f6db93180424e20c86465: `RotationAnalysis.cs`, `HpCheck.cs`, `Deaths.cs`, `ProtocolHealth.cs`, `Aion2FrameDecoder.cs`, Protokollnotizen und `StreamOverlayServer.cs`. MIT, Copyright (c) 2026 SkeeveTV. Lizenztext unter `licenses/SkeeveAN-MIT.txt`. Die zusätzlichen begrenzten Frame-Leser sind für Rust angepasst. Keine fremden Statistikarchive oder zusätzlichen Spielgrafiken wurden daraus übernommen.
- nathantek/aion2assistant, Tree 08e36a88ce497b21e21914eea79bb234d9977b3a: Ideen für getrennte Capture-Diagnose und Kontrastprüfung. Keine Implementierung kopiert. Das Projekt nennt GPL-3.0 für Code und eigene Hinweise für Spieldaten.
- Klauensprung/Wingbeat-Aion-Dps-Meter, Tree 73e69612bba9d0c40607add11be022f710bf4c07: Konzepte für Cooldowns, Gruppenrotation und getrennte Cast-/Treffer-/Verteidigungszahlen. **Kein Code kopiert oder übersetzt.** Laut THIRD-PARTY-NOTICES ist keine eigene Projektlizenz gewählt.
- neophrythe/Aion-2-Sentinel-DBM-and-Damage-Meter-, Tree 3701bf42dfb342f767a900bec482c683f3cec3c4: Konzept, Bosswarnungen nur aus belegten Signalen abzuleiten. Keine Implementierung veröffentlicht und kein Code übernommen. Kein OCR-System, keine fremde private Bossbibliothek und keine erfundenen Mechanikpläne.

Der bestehende A2Tools-Kern ist als nachvollziehbarer Quellbestand unter `vendor/a2tools-core` enthalten. Ausgangsrevision, Lizenz und lokale Änderungen stehen dort in NOTICE.md. So sind die Änderungen an den Aggregattypen beim Bauen reproduzierbar. Parser-IDs und einzelne Drahtwerte behalten ihre Typen. Schadens-/Heilungssummen, Detailantworten und gespeicherte Summen verwenden 64 Bit. Ältere, bereits begrenzte Messungen werden nicht nachträglich repariert. Die Sprachtests des Quellbestands prüfen die beiden in dieser Anwendung ausgelieferten Tabellen, Deutsch und Englisch.

## Neue Bedienung

### Kampfbericht

Der aufklappbare Abschnitt **Rotation, Bossanalyse und Replay** zeigt:

- Skill-Nutzungen aus eigenständigen Nutzungsframes. Treffer, DoTs und Heilungsticks erzeugen keine künstlichen Casts.
- Gruppen- oder Einzelansicht, Opener, beobachtete Nutzungen/min, mittlere Wiederholungsabstände.
- Gemeinsame Zeitachse mit beobachteten DPS-Abschnitten, Skill-Icons, Buff-Intervallen, Todes- und Stagger-Markern. Zeitfenster lassen sich verschieben und auf 10 oder 30 Sekunden begrenzen.
- Buff-Beiträge nach Wirker und eigener Uptime. Überlappende Intervalle zählen in der Gesamtuptime einmal. Instanzbezogene Endereignisse beenden einzelne Buff-Anwendungen. Ohne erkanntes Ende bleibt die gemeldete Dauer eine Schätzung.
- Boss-Cast-Signale getrennt von Treffern und Verteidigung. Flächentreffer sind keine zusätzlichen Casts.
- Letzte beobachtete Treffer vor einem erfassten Tod, mit Wiederbelebung, falls später positive HP beobachtet wurden. Keine automatische Behauptung einer Todesursache.
- HP-Plausibilität in einem gemeinsamen beobachteten Fenster. HP-Anstieg, lange HP-Lücke, Datenlücke oder Overkill verhindern ein positives Urteil. Eine Lücke wird nicht automatisch zum Schild erklärt. Ein passender Gesamtwert beweist keine korrekte Spielerzuordnung.

Die Ereignisansicht kann bis zu zwei Sekunden vor dem ersten und drei Sekunden nach dem letzten Schaden umfassen. Reset-/Zonengrenzen begrenzen diesen Bereich. **Das gemeinsame DPS-Zeitfenster bleibt unverändert.** Ältere Kämpfe ohne Ereignisse bleiben lesbar und zeigen einen verständlichen Hinweis.

Der Performance Coach ergänzt Unterschiede in Nutzungen/min nur bei vorhandener vergleichbarer Ereignisaufzeichnung. Er behauptet daraus keine Ursache oder ideale Rotation.

### Replay

Play/Pause, 0,5–4×, Zeitregler, Pfeiltasten ±5 s, Leertaste, Zoom per Mausrad, Verschieben und Einpassen. Gezeigt werden Gruppe und Boss, kurze beobachtete Spuren, Todeszustand und gemeldete Blickrichtung. Positionen sind sporadische Servermessungen. Nur zwischen Messungen mit höchstens drei Sekunden Abstand wird linear interpoliert. Nach drei Sekunden ohne neue Messung verschwindet ein Punkt. Ziel-/Richtungsvektoren eines Skills werden nicht als Weltposition gelesen. Flächenradien und nicht beobachtete Laufwege werden nicht erfunden.

### Overlay

Unter **Einstellungen → Kampfhilfen**:

- Optionaler Boss-Cast aus einem erkannten Windup mit gemeldeter Dauer. Abschluss und Unterbrechung sind nicht garantiert erfasst. Es gibt keine wiederkehrenden Mechanik-Timer.
- Bis zu acht Cooldown-Skills, wählbar über eine Suche im Skill-Katalog, alternativ über IDs. Die Einheit der Rohwerte ist nicht bestätigt. Ohne Faktor gibt es keine Sekundenanzeige. Ein manuell gewählter Faktor liefert mit `~` markierte Schätzwerte. Nullmeldungen/Resets überschreiben alte Werte. Charakter-/Entitätswechsel löschen alte Cooldowns. Alte Meldungen laufen aus. Live-Countdowns verwenden eine monotone Uhr, Offline-Replay die Aufnahmezeit. Die neueste Meldung einer bekannten Skillfamilie, einschließlich Null/Reset, hat Vorrang.
- Hoher Kontrast erzwingt einen deckenden Hintergrund. Die Vorschau berechnet einen konservativen Haupttextkontrast über schwarzem und weißem Untergrund. Klassenicons ergänzen die Farben.

### Diagnose

Paketfamilien werden getrennt gezählt, einschließlich unbekannter Opcodes und nicht unterstützter Layouts. Ein möglicher Protokollbruch wird nur gemeldet, wenn eine zuvor etablierte Schadensfamilie mehrere vollständige Fenster ausbleibt, während aktuelle Skill-Nutzungen eintreffen. Empfang allein bestätigt keine korrekte Messung. Leerlauf allein erzeugt keinen Protokollbruch. Die kopierbare Diagnose enthält keine Charakternamen, Adressen, Mitschnittpfade oder Schlüssel.

### Geschützte OBS-Freigabe

Ein **separater, ausschließlich lesender Listener** läuft standardmäßig auf 127.0.0.1:8788. Zunächst ist die Freigabe deaktiviert. Einschalten und Link kopieren unter Einstellungen. Die Stream-Ansicht lädt keine externen Ressourcen und gibt weder Dashboard-Aktionen noch Historie, rohe Ereignisse oder interne Spieler-IDs frei. Namen sind standardmäßig durch Klassenbezeichnungen ersetzt. Acht gleichzeitige HTTP-Anfragen sind erlaubt. Polling fragt jeweils den neuesten Snapshot ab und baut keinen Nachrichtenrückstau auf.

Für zwei PCs mit `--stream-listen 0.0.0.0` starten. Nur Loopback/private/link-local Gegenstellen und passende Host-Adressen werden bedient. `--stream-port` ändert den Port. Die Schlüsselverwaltung ist nur über eine Loopback-Verbindung zum Dashboard möglich. Ein kryptografisch zufälliger Schlüssel gilt bis zum Neustart oder **Schlüssel erneuern**. Deaktivieren macht vorhandene Links sofort unbrauchbar. Das bestehende `/overlay` bleibt die lokale Vorschau.

## Grenzen und Abnahme

Die Ereigniserfassung ist auf 50.000 Ereignisse und zwei Stunden begrenzt. Verdrängte Daten und TCP-Lücken markieren betroffene Auswertungen als unvollständig. Diese Grenzen sind keine Garantie, dass vor Mitschnittbeginn alle Ereignisse vorhanden waren.

Erforderliche Ingame-Gegenprüfung: EU-Frames für Nutzungen und Unterbrechungen, Boss- und Spieler-HP, Buff-Ende mit mehreren Wirkern, Tod/Wiederbelebung, Skillvarianten und Cooldown-Zeiteinheit, lange Kämpfe, Zonenwechsel und KDE/Wayland-Darstellung. Synthetische und Softwaretests ersetzen diese Abnahme nicht.

Der offene PR #33 zur Behandlung vorhandener Community-Archive ist ein getrenntes Vorhaben. Dieser Branch ändert daran nichts und fügt keine weiteren fremden Referenzdaten hinzu.

Builds und CI verwenden Rust 1.88.0, die erklärte Mindestversion und lokal erfolgreich geprüfte Release-Toolchain. Rust 1.99.1 zeigte in dieser Umgebung beim ThinLTO-Linken undefinierte Symbole. Das bestehende PR #33 dokumentiert denselben Fehler auf unverändertem main. LTO und Optimierung bleiben eingeschaltet.
