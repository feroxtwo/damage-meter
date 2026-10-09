# Combat Studio: Oberfläche und Vergleich

Stand: 9. Oktober 2026. Grundlage ist main nach PR #26 (`5f30ad9`). Die Recherche betrachtet öffentlich zugängliche Produktbeschreibungen und offizielle Screenshots. Angaben anderer Anbieter sind deren Aussagen, keine unabhängig bestätigte Genauigkeitsmessung. Keine Anmeldung, kein Kauf, kein Upload eigener Kämpfe.

## Was andere Meter anbieten

| Produkt und Quelle | Angebot / Finanzierung | Sichtbare Bedienidee | Bei uns vorhanden / Grenze |
| --- | --- | --- | --- |
| [A2Tools](https://a2tools.app/) | Kostenloser Open-Source-Meter; Unterstützer und Sponsoring. Das dortige „Profiteer Premium“ betrifft ein anderes Angebot. | Gruppenrangliste, ausgewählter Spieler, Kurve und Skilltabelle in einem Analysefenster; gespeicherte Bosskämpfe mit Filtern. | Gruppen- und Skillanalyse, OBS, Verlauf und Vergleiche vorhanden. Unsere Zeitlinien zeigen beobachtete Treffer/Ticks; daraus werden keine Casts oder Rotationsempfehlungen abgeleitet. |
| [JaMeter](https://www.jameter.net/en) | Als kostenlos beschrieben; kein belegtes Premium-Abo für den Meter. | Spieler oben auswählen, Skillanteile direkt in der Tabelle, Damagekurve im Kontext; Training und KP-bezogene Einordnung. | Training, Skillanteile, lokale KP-Vergleiche vorhanden. Keine Übernahme der S+–D-Wertungen, Cooldown-Alarme oder Spawn-Timer ohne entsprechende verlässliche Daten. |
| [Abyss Logs](https://abysslogs.com/) | Freier Meter und Analyse; Unterstützer bekommen laut FAQ mehr Cloud-Speicher. | Kampf oder Dungeon als zusammenhängender Bericht; Skills, Buffs und Community-Statistik mit kurzen Wegen. | Dungeon-Gesamtauswertung mit aggregierten Skills und Verlauf vorhanden. Vergleichsdaten bleiben lokal; kein automatischer Cloud-Upload und keine vorgetäuschte weltweite Rangliste. |
| [Details!](https://www.curseforge.com/wow/addons/details) | Öffentlich angebotenes WoW-Addon; Patreon-Unterstützung mit exklusiven Inhalten. Eine vollständige aktuelle Liste zahlungspflichtiger Extras wurde nicht geprüft. | Klare Optionsnavigation, bevorzugte Anzeigen, Encounter-Details und Diagramm-/Zeitlinien-Plugins. | Schaden, Heilung und erlittener Schaden auswählbar; gespeicherte Kämpfe und Favoriten. WoW-spezifische Talent-, Ausrüstungs- und Raid-APIs sind nicht auf unsere Paketdaten übertragbar. |
| [Warcraft Logs](https://www.warcraftlogs.com/subscribe) | Öffentliches Abo-Angebot mit Archivzugriff und zusammengefasster Analyse mehrerer Reports; Unterstützervorteile sind nicht pauschal alle Analysefunktionen. Die vollständige Seite war im Webabruf mit HTTP 403 gesperrt, ihre offizielle Suchindex-Fassung zugänglich. | Ein Bericht ist ein Analysearbeitsplatz; längere Historie und mehrere Versuche gehören zusammen. | Einzelkampf-Vergleich und persönliche Entwicklung vorhanden. Keine Aussage, dass unsere lokal beobachteten Spieler eine repräsentative globale Vergleichsgruppe ergeben. |

Visuell angesehen: offizielle A2Tools-Spieler-/Skillansicht und JaMeter-Kampfanalyse. Produktbeschreibungen geprüft: alle fünf oben. Der Shop `aion2dpsmeter.com/pages/features` leitete auf eine Passwortseite um; dessen Funktionen und Preise bleiben unbestätigt und wurden nicht als Entscheidungsgrundlage verwendet.

## Daraus abgeleitete Änderungen

Die Bewertung der Gestaltung ist unsere Interpretation: Hauptproblem war die fehlende Orientierung zwischen vielen gleich gewichteten Flächen. Viele leistungsfähige Auswertungen waren erst durch Scrollen und Aufklappen zu entdecken.

- Feste Navigation am Desktop mit kurzen Beschreibungen; dieselben fünf Bereiche als kompakte Navigation am Handy. Bestehende Links `#live`, `#runs`, `#stats`, `#skills`, `#settings` bleiben gültig.
- Eigene visuelle Identität mit Violett als Studio-Akzent, klarer Typografie und einer gerahmten Live-Begegnung. Aether und Ember bleiben eigenständige Varianten. Klassenfarben kennzeichnen weiterhin Spieler.
- Live zeigt die eigene Leistung im Vordergrund, daneben den beobachteten Burst-Verlauf und die Gruppenwerte. Gruppenergebnis und Training erhalten erkennbare Arbeitsbereiche. Ein direkter Weg führt zu gespeicherten Kampfberichten.
- Der Kampfbericht bietet sechs sichtbare Sprungziele: Gruppe & Skills, Schadensverlauf, Entwicklung, Vergleichen, Notizen und Messdetails. Ein Klick öffnet bei Bedarf den Bereich, scrollt dorthin und setzt den Tastaturfokus. Die Navigation bleibt beim Scrollen erreichbar; sie filtert oder ersetzt keine Daten.
- Skilltabellen zeigen gemessene Schadens- beziehungsweise Heilungsanteile zusätzlich als Balken. Die bestehenden Zahlen, Sortierung, Suche und Exporte bleiben maßgeblich.
- Einstellungen zeigen die echte OBS-Ansicht über `/overlay` mit Live-Daten und gespeicherten Einstellungen. Der zusätzliche Polling-Client wird nur dort geladen und beim Verlassen entfernt. Dies ist keine Vorschau der nativen Fensterposition. Bei hoher Skalierung können Inhalte den Rahmen überschreiten; die separat zu öffnende OBS-Ansicht bleibt verfügbar.
- Ein gemeinsamer Export-Einstieg mit Formatauswahl bleibt erhalten. Keine zusätzlichen Exportknöpfe je Format.

## Prüfung und Grenzen

Browserprüfungen umfassen Berichtnavigation mit Fokuswechsel, Laden/Entladen der Vorschau, bestehende Exporte und Privatsphäre, große Berichte, drei Designs und schmale Layouts bis 320 px. Bilder der Browserprüfungen verwenden synthetische Testdaten, die ausschließlich im Testskript liegen. Die Anwendung bekommt keine Demonstrationswerte oder künstlichen Kurven.

Die Änderung betrifft das Web-Dashboard. Das native Spieloverlay und seine Fensterbedienung werden damit nicht neu gestaltet. Messung, Parser und Datenbank bleiben unverändert. Veröffentlichbare Cloud-Logs, bestätigte Cast-Zeitlinien, globale Perzentile und Cooldown-Assistenten würden eigene Daten- und Integrationsarbeit benötigen.
