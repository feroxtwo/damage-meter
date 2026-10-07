# Kampfanalyse und Bedienung in 0.3.0

> Historische Prüfung/Funktionsbeschreibung. Den aktuellen Teststand und spätere Änderungen dokumentiert [Releaseprüfung 0.3.1](RELEASE_REVIEW_0.3.1.md).

## Parser und Messwertprüfung

A2Tools 2.0.52 wird auf `d3cf6f92533721939f4b4454d163c6d1dd820666` festgelegt. Gegenüber unserer vorherigen Version korrigiert Upstream unter anderem Bodenflächen-/Beschwörungszuordnung, Ressourcenwiederherstellung gegenüber Heilung, Party-Erkennung und Treffermerkmale. Die neuen deutschen/englischen Skillnamen sind eingebunden. Vorhandene gespeicherte Kämpfe bleiben lesbar.

Automatisierte Prüfungen belegen die Verarbeitung definierter Daten, keine allgemeine Genauigkeitsquote. Für einen echten Messvergleich:

1. Aufnahme vor dem Kampf starten. Eigene Spielidentität und Zielerkennung prüfen.
2. Einen einzelnen Boss oder eine Trainingspuppe ohne Zielwechsel messen. Nach Ende die spielinterne Kampfanalyse sichern, mit vollständigen exakten Skill-Summen und derselben Messzeitspanne.
3. Aufnahme offline auswerten:
   ```sh
   aion2-meter replay kampf.a2mcap --output replay.json
   ```
4. Aus der Ingame-Auswertung eine CSV mit allen angezeigten Schadens-/Heilungsskills erstellen. Nicht K/M-gerundete Anzeigen als exakte Zahlen übernehmen. DoT-Zeilen anhand der Replay-Skillcodes zuordnen:
   ```csv
   kind,code,is_dot,total
   damage,11010000,0,123456
   heal,1900001,0,654321
   ```
   Diese Zeilen sind nur Formatbeispiele, keine Messwerte.
5. Actor- und Target-ID aus `replay.json` wählen und vergleichen:
   ```sh
   python3 scripts/validate-combat.py replay.json ingame.csv --actor 123 --target 456 --tolerance-percent 0.5 --output vergleich.json
   ```

Der Vergleich meldet Abweichungen je Skill und Gesamtsummen, zusätzliche/fehlende Skills und die verwendete Parser-Version. Exit-Code 0 bedeutet innerhalb der gewählten Toleranz, 1 bedeutet Abweichung, 2 fehlerhafte Eingaben. DPS-Zeitmodelle, Treffermerkmale und Buffs werden damit nicht validiert. Mehrere Klassen, DoTs, Beschwörungen, reine Heilung und Wiederverbindungen müssen in getrennten realen Läufen überprüft werden. Aufnahmen und rohe Replay-Berichte enthalten Namen, Chat und Verbindungsdaten. Nur lokal prüfen oder bewusst bereinigen.

## Versuche und Resets

Leerlauf-Reset und Wipe-Reset sind standardmäßig aus. Leerlauf ist auf 15–900 Sekunden begrenzt und berücksichtigt die zuletzt erfasste Schadensaktivität im Segment. Das Dashboard bietet 15/30/60/120 Sekunden an. Auto-Reset ist während Training aus.

Die Wipe-Erkennung ist absichtlich konservativ: Bossmodus, mindestens zwei erfasste Spieler einschließlich des lokalen Spielers, alle beobachtet tot, mindestens zwei Sekunden Totzustand und fünf Sekunden ohne Schaden, vorher gemeldete HP höchstens 80 %, danach mindestens 98 % und mindestens 20 Prozentpunkte Anstieg. Geschätzte HP, Zielwechsel oder ein beobachteter Bosstod reichen nicht. Neue Schadensaktivität nach dem erfassten Totzustand sperrt Wipe-Erkennung für diesen Versuch, damit die bis zum Reset gespeicherten Tod-Markierungen nach einer Wiederbelebung nicht erneut verwendet werden. Nicht erfasste Gruppenmitglieder und fehlende HP-/Tod-Pakete können die Erkennung verhindern. Heilung eines Bosses nach dem Tod aller erfassten Spieler kann weiterhin ähnlich aussehen. Der Schalter ist deshalb optional.

Vor einem Reset werden auch kurze Versuche mit eigenem Schaden gespeichert. Ein Speicherfehler verhindert unseren Reset. Abschlussgründe bleiben am Bericht erhalten. Ein bereits erfasster Kill wird nicht zu einem Wipe. Regeln gelten auch im Replay, wenn entsprechende lokale Einstellungen verwendet werden. Im isolierten Replay sind sie standardmäßig aus.

## Teilen und Vergleichen

PNG-Berichte enthalten die gesamte Spieler-/Skillliste einschließlich Heilung. Lange Berichte werden in mehrere nummerierte PNGs aufgeteilt. Die Namen anderer Spieler sind standardmäßig anonymisiert, der eigene Charakter bleibt benannt. Über die Auswahl neben dem PNG-Button lässt sich der Bericht auf einen Spieler beschränken; voreingestellt ist der eigene Charakter. Notizen, Tags, Verbindungsdaten und interne IDs werden nicht exportiert. Chatzeilen sind einzeilig, enthalten höchstens 200 Unicode-Zeichen und folgen der ausgewählten Kennzahl. Das ist eine lokale Längengrenze, keine Zusage für jedes regionale Chatlimit.

Im gespeicherten Kampf können zwei Spieler mit Skill- und Buff-Zeitlinien nebeneinander stehen. Das gemeinsame Diagramm zeichnet Intervall-DPS für alle Spieler mit Legende. Es beruht auf kumulativen Beobachtungen im 500-ms-Takt, nicht auf exakten Einzelereignissen. Live-Details lassen sich mit einem zweiten Spieler anheften. Die beiden Momentaufnahmen entstehen nacheinander und werden bei einem erkannten Ziel- oder Versuchswechsel nicht kombiniert.

Treffer/Ticks werden weiter als Treffer/Ticks bezeichnet. Vollständige Erfassung der Treffermerkmale ist unbekannt. Positive Werte sind beobachtete Anteile; null/fehlende Werte erscheinen als „—“, keine nachgewiesenen Merkmale werden nicht als gemessene 0 % ausgegeben. Dies ist keine nachträglich erfundene Qualitätsquote. Der aktualisierte Parser liefert auch Block, Perfektblock, Ausdauer, Regeneration, Miss und Resist. Resist bezeichnet widerstandene Effekte, nicht automatisch schadensfreie Treffer. Altdaten können diese Werte nicht belegen.

## Design und Installation

Midnight, Aether und Ember sowie kompakte Zeilen stehen im nativen Overlay, Dashboard und OBS-Overlay bereit. Sie werden mit den Profilen gespeichert. Klassenspezifische Balkenfarben bleiben erhalten.

`./scripts/package-linux.sh` erstellt aus der Release-Binary ein `.deb`, `.rpm` (mit `rpmbuild`) und ein Archiv mit `scripts/install-binary.sh`, dazu SHA-256-Prüfsummen. Die Pakete enthalten keine Datenbank und überschreiben keine Benutzerdaten. Debian setzt `cap_net_raw` beim Konfigurieren, RPM speichert die Capability im Dateimanifest. Kein Betrieb der App als root. Systempakete enthalten KWin- und Shortcut-Skripte unter `/usr/lib/aion2-meter/`. Auf KDE bei Bedarf:

```sh
/usr/lib/aion2-meter/install-kwin-rule.sh
BIN=/usr/bin/aion2-meter /usr/lib/aion2-meter/install-shortcuts.sh
```

Die Debian-Abhängigkeit wird aus den tatsächlich benötigten GLIBC-Symbolversionen der Binary ermittelt, RPM ermittelt seine Laufzeitabhängigkeiten automatisch. CI baut auf Ubuntu 22.04 für x86_64 und stellt Pakete als Actions-Artefakte bereit. Der Tag-Workflow erstellt erst nach einem passenden `v0.3.0`-Tag einen **Release-Entwurf**. Der Maintainer entscheidet über dessen Veröffentlichung. Ein PR allein erzeugt kein öffentliches Release.

Die Updateprüfung kontaktiert GitHub ausschließlich auf Klick, mit Timeout und Größenlimit. Sie lädt keine Programme herunter und installiert nichts. Ohne veröffentlichtes Release wird das ausdrücklich gemeldet. Ubuntu/Debian verwenden das `.deb`, Fedora das `.rpm`, Bazzite das RPM über `rpm-ostree install` mit anschließendem Neustart. Das Archiv kann ohne Rust/Node im Benutzerverzeichnis installiert werden. Paketinstallation/Upgrades und native KDE/Wayland-Fenster bleiben zusätzlich auf den Zielsystemen zu prüfen.

## Automatisierte Validierung

64 lokale Rust-Tests, 123 Tests des aktualisierten Upstream-Parsers, 25 Chromium-Prüfungen und vier Tests des Ingame-Vergleichswerkzeugs sind erfolgreich. Zusätzlich geprüft: Format, Clippy ohne Warnungen, Rust 1.88, Release-Binary mit API/Replay-Smoke-Test, Debian-/RPM-/Archivbau und RPM-Dateicapability. Browserbilder verwenden synthetische Daten. Reale Kampfgenauigkeit, native Fenster unter KDE/Wayland sowie Paketinstallation und Upgrade auf den Zielsystemen sind noch nicht vor Ort nachgewiesen.
