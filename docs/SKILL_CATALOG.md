# Fähigkeiten, Sprachen und Symbole

Der Meter enthält einen lokalen DE/EN-Katalog mit 364 Hauptfähigkeiten aller neun Klassen einschließlich Faustkämpfer sowie allgemeinen Fähigkeiten. Der Tab **Fähigkeiten** sucht in beiden Sprachen, nach primärer ID und nach Varianten-ID. Klassenfilter, weitere Paket-IDs/Effekte und Seiten mit höchstens 80 Einträgen halten die Liste bedienbar.

Unter **Overlay → Fähigkeits- und Klassennamen** lässt sich Deutsch oder Englisch wählen. Die Wahl wird mit Einstellungen und Profilen gespeichert. `--lang en` bestimmt die Voreinstellung bei fehlender gespeicherter Sprachwahl; vorhandene Einstellungen haben Vorrang. Das übrige Dashboard bleibt deutsch.

Suche, Klassenfilter, zusätzliche Paket-IDs und aktuelle Katalogseite bleiben im selben Browser auch nach einem Neuladen erhalten. **Zurücksetzen** stellt die ungefilterte erste Seite wieder her. Bei fehlenden Treffern erscheint eine Erklärung. Beschädigter oder gesperrter Browserspeicher verhindert die Nutzung nicht; gleichzeitiges Öffnen des Tabs teilt sich eine Kataloganfrage.

**Tastatur:** `/` fokussiert die Suche im Fähigkeitentab, in der Kampfbibliothek oder im geöffneten Skilldialog. Beim Schreiben in Eingabefeldern bleibt `/` ein normales Zeichen. `Esc` leert zuerst eine befüllte Suche; im Dialog schließt ein weiteres `Esc` wie bisher den Dialog. Die Kürzel stehen auch im Hinweis der Suchfelder.

Kopieren aus einem Dialog verwendet bei verweigerter Clipboard-API einen lokalen Fallback innerhalb dieses Dialogs und stellt den vorherigen Fokus wieder her. Wenn beide Kopierwege scheitern, erscheint eine Meldung zur Zwischenablage statt eines Verbindungsfehlers. Temporäre Kopierfelder werden auch im Fehlerfall entfernt.

## Umfang und Zuordnung

| Bestandteil | Umfang / Verhalten |
|---|---|
| Hauptkatalog | 364 Einträge; jeder mit DE- und EN-Namen |
| Deutsche Quellnamen | 323 aus Questlog DE |
| Faustkämpfer | 41 deutsche Community-Übersetzungen, sichtbar gekennzeichnet; englische NCSOFT-Namen aus Questlog `en-nc` |
| Bestehende Paket-Namen | 9.268 IDs je alter Sprachdatei weiterhin verfügbar; zusätzliche Namen erscheinen über „Weitere Paket-IDs / Effekte“ |
| Varianten mit Hauptzuordnung | 4.212 IDs; exakte englische Namensidentität, bei Mehrdeutigkeit zusätzlich exakte bekannte ID-Familie |
| Skill-Symbole | 353; alle Hauptfähigkeiten mit Quell-Icon abgedeckt |
| Klassensymbole | Alle neun Klassen, auch natives Overlay und OBS-/Browser-Overlay |
| Allgemeine Einträge ohne Quell-Icon | Elf; sichtbares `?`, keine erfundene Zuordnung |
| Neue / unbekannte IDs | Gespeicherter Name oder `#ID`; kein geratenes Symbol |

Gleiche Namen verschiedener Klassen werden nicht allein nach Namen zusammengelegt. Die tatsächliche Paket-ID bleibt im Bericht erhalten, `skill_id` kennzeichnet lediglich die zugeordnete Hauptfähigkeit. DoT-/HoT-Unterscheidung bleibt unverändert. Die alten Sprachdateien bleiben als Rückfall für Pets, Effekte und sonstige Paket-IDs erhalten; fehlende Übersetzungen fallen auf den verfügbaren Namen zurück.

Die API ergänzt `names`, `icon`, gegebenenfalls `skill_id` und `name_source_de`. Live-Skilldetails und historische Berichte verwenden dieselbe Präsentation. Es gibt keine Datenbankmigration: numerische Werte, Zeitpunkte, Kampf-/Spieleridentitäten und gespeicherte Originalnamen werden nicht umgeschrieben. Auswertungen, Spieler-/Kampfvergleiche, Trefferzeitlinien und PNG-Berichte zeigen lokale Skillbilder; Rankings und Gruppen zeigen lokale Klassensymbole. JSON-/PNG-Ausgaben benutzen die gewählte Sprache und erhalten die bestehende Anonymisierung.

## Offline-Paket und Wartung

`/api/skills` liefert den eingebetteten Katalog und weitere bekannte Paketnamen. `/assets/icons/{name}` liefert ausschließlich im Manifest enthaltene WebP-Bilder; beliebige Dateipfade werden nicht gelesen. Darstellung und PNG-Erstellung benötigen keine Verbindung zu Wakayashi oder Questlog. Der gesamte deduplizierte Bilderblock ist 325.474 Bytes groß; native Class-Texturen werden einmal geladen.

`data/skills/catalog.json` enthält die Identitäten und überprüften Alias-IDs. `icons.json` beschreibt Ausschnitte aus `icons.bin` und deren SHA-256. `classes.rgba` enthält neun native Klassensymbole. Herkunft und Rechtezuordnung: [NOTICE](../data/skills/NOTICE.md).

Das Wartungsskript `scripts/import-skill-catalog.py` liest bereits heruntergeladene Quellen; es umgeht keine Zugriffsbeschränkungen und lädt nichts selbst. Es benötigt Pillow und die im Skript beschriebenen JSON-Manifeste/Originalbilder. Quellen müssen beim nächsten Update erneut anhand der sichtbaren Katalogseiten gesammelt und auf Vollständigkeit geprüft werden. Beispiel:

```sh
python scripts/import-skill-catalog.py --sources /pfad/zu/quellen --wakayashi /pfad/zu/wakayashi-klassen.json
cargo test --locked skills::tests
node scripts/test-skill-ui.cjs
```

Die Zuordnung anhand realer AION-2-Pakete, aktuelle Spielübersetzungen und Ingame-Messgenauigkeit sind gesondert zu prüfen: [Ingame-Abnahme](INGAME_ACCEPTANCE.md).
