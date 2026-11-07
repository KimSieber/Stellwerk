# Übergabe für v0.3 – Stellwerk (Stand 07.11.2026)

Einstieg für einen neuen Chat. Bitte zuerst dieses Dokument lesen, dann
[`docs/v0.2 Entwicklungsdoku.md`](../v0.2%20Entwicklungsdoku.md) (was in v0.2 dazukam),
[`docs/v0.1 Entwicklungsdoku.md`](../v0.1%20Entwicklungsdoku.md) (Grundlagen),
[`docs/KONZEPT.md`](../KONZEPT.md) und [`docs/ARCHITEKTUR.md`](../ARCHITEKTUR.md) – für v0.3 besonders ARCHITEKTUR Kap. 4.2, 7, 8 und 12.

## 1. Projekt in einem Satz

**Stellwerk** ist eine Desktop-App (Tauri 2 · Rust · React/TypeScript) für kontrollierte Transportstrecken von Dateien: Quelle → lokaler Spiegel (Test) → Server per FTPS, oder reine lokale Kopierstrecken – mit Vergleich nebeneinander, bewusster Auswahl, Regeln, Schlüsselbund und Protokoll.

## 2. Orte

| Was | Wo |
|---|---|
| Lokales Verzeichnis | `/Users/kim/Dropbox/Dev/GitHub/Stellwerk` |
| GitHub | https://github.com/KimSieber/Stellwerk (Branch `main`) |
| App-Identifier | `de.sieber-bw.stellwerk` |
| Laufzeitdaten der App | `~/Library/Application Support/de.sieber-bw.stellwerk/` |
| Webseite (geplant, noch nicht angelegt) | https://stellwerk.sieber-bw.de |
| Vorbild Drag-and-drop | PaperTrail Compare, `src/hooks/useDragDropTarget.ts` |
| Vorbild „Über …“ | Mailwerk, `Mailwerk/Views/About/AboutView.swift`, `Services/About/AppInfo.swift` |

## 3. Arbeitsregeln (verbindlich)

- **Claude committet niemals.** Ob ein Stand commit-reif ist und der Commit selbst bleiben immer bei Kim. Claude liefert nach jedem Durchgang einen **Commit-Text**.
- **Testpflicht:** Jede Funktion und jede Funktionsstrecke wird durch einen automatischen Test abgedeckt. `npm run pruefen` muss grün sein (Abdeckung 100 % Funktionen/Zeilen, harte Grenze). Neue Tests mit **Gegenprobe** (Fehler absichtlich einbauen → Test wird rot).
- Antworten und Oberfläche auf **Deutsch**; Code-Bezeichner überwiegend deutsch.
- **In kleinen Schritten:** höchstens zwei Arbeitspakete, dann Rückmeldung und Kims Test abwarten.
- Unklares vorher im Dialog klären; bei Konzeptlücken die vorsichtigere Variante wählen und **ausdrücklich zur Prüfung vorlegen**.
- **Keine Automatismen** bei Regeln/Ausnahmen – alles bewusst gesetzt.
- Kim prüft jede Änderung in der laufenden App (`npm run tauri dev`) und meldet Auffälligkeiten mit Screenshot.
- **Terminal-Befehle für Kim nie als Block** zum Kopieren, wenn sie nicht zusammen laufen sollen: **ein Befehl pro Schritt**, Ausgabe abwarten, dann der nächste. Keine `#`-Kommentare in Befehlszeilen (zsh führt sie als Befehl aus).

### Technischer Ablauf mit Claude (bewährt in v0.1/v0.2)

- Claude arbeitet in einer Linux-Umgebung (Klon des GitHub-Repos), baut und testet dort: `cargo test`, `cargo llvm-cov`, Vitest, Playwright-Aufnahmen einer Vorschau-Seite mit nachgebildeter Rust-Seite (`mockIPC`). Für Tauri unter Linux einmalig die WebKitGTK-Pakete installieren.
- **Vor dem Bauen** den Stand in Kims Ordner mit dem GitHub-Stand bzw. dem zuletzt gelieferten Stand vergleichen (Prüfsummen) – Kim kann zwischendurch selbst geändert haben.
- **Übertragung** in Kims Ordner über die Claude-App: geänderte Dateien **einzeln an ihren Platz** schreiben (bis 50 je Übertragung), danach Prüfsummen-Vergleich. Keine Archive, kein Ordner `_to_delete/`. Mehr als 50 Dateien: vorher melden.
  - Für jede Übertragung einen **neuen** Bereitstellungsordner verwenden – eine erneut bereitgelegte Datei unter demselben Pfad kam einmal in alter Fassung an.
  - PNG/SVG bekommen bei der Übertragung Herkunfts-Metadaten (C2PA) – Prüfsummen weichen ab, Bilddaten sind gleich (bei PNG die dekomprimierten IDAT-Daten vergleichen).
- **`npm install` immer auf dem Mac** (Kim), nie in der Linux-Umgebung.
- **`package.json` und `package-lock.json` nie blind überschreiben:** Kims npm (Version 12) setzt Markierungen anders, und `package.json` enthält `"allowScripts": { "fsevents": false }`. Vor einer Änderung Kims Fassung holen und nur die eigene Änderung einarbeiten.
- macOS-spezifischer Code (`schluesselbund.rs`) zusätzlich gegen `aarch64-apple-darwin` prüfen.
- **Kein `git` über die Geräte-Shell** auswerten: Sie läuft in einer Linux-VM; macOS legt Umlaute in Dateinamen zerlegt ab (NFD), was dort Git ohne `precomposeunicode` als unversionierte Datei meldet – auf Kims Mac ist alles in Ordnung.
- Ist der Mac nicht verbunden: Archiv im Chat bereitstellen – sobald die Verbindung wieder steht, lieber einzeln schreiben.

## 4. Stand v0.2 (abgeschlossen)

- **Bedienbar:** alles aus v0.1, dazu auf der Startseite „Vergleichen“ je lokaler Stufe → Vergleichsansicht mit drei Kästen Quelle | Aktion | Ziel, Häkchen, Filter (Start „Alle“), aufklappbarer Baum, Summen, Warnungen, Regel-Herkunft im Tooltip, Datei-Icons; Rechtsklick → „Regel setzen“ für Stufe oder Strecke inkl. „Ersetzen durch …“ und „Regel entfernen“, danach automatisch neu verglichen.
- **Gesperrt sichtbar:** „Übertragen …“ (v0.3), „Vergleichen“ bei FTPS-Zielen (Manifest, v0.4).
- **Tests:** 182 Kern-, 31 Befehls-, 204 Frontend-Tests; Abdeckung Funktionen/Zeilen 100 %.
- **Entwicklungsumgebung auf Kims Mac neu:** Node 26 und rustup über Homebrew (Details: v0.2 Entwicklungsdoku Kap. 2).

## 5. Entscheidungen aus v0.2 (zusätzlich zu KONZEPT/ARCHITEKTUR und Übergabe v0.2)

1. **Vergleichs-Auslegung von Kim bestätigt** (Schützen fasst am Ziel nie etwas an, Nur anlegen legt nur Fehlendes an, Ordner mit bleibendem Inhalt bleiben, Konflikte/Konfliktkopien/„nur online – überspringen“ nicht vorausgewählt).
2. **Auswahl:** Häkchen nur, wo vorausgewählt; Ordner wirkt auf Inhalt; neue Elternordner werden mit angehakt; zu löschender Ordner verliert das Häkchen, wenn darin etwas bleibt.
3. **Sortierung:** Ordner zuerst, deutsch (Groß/Klein egal, Umlaute wie Grundbuchstabe, ß wie ss), Zahlen nach Wert.
4. **Je Stufe höchstens ein laufender Vergleich und eine offene Sitzung**; ein neuer Vergleich bricht den alten ab. Die Ansicht startet je Öffnen genau einen Auftrag (StrictMode-fest) und gibt nur über „Zurück“ frei.
5. **Darstellung:** drei Kästen mit Legenden-Überschrift wie die Ordnerkästen oben; Häkchen im Aktions-Kasten; Start mit Filter „Alle“; Datei-Icons einfarbig grau (Lucide).
6. **Kontextmenü:** Ebene wählbar (Stufe/Strecke); Muster immer **genau dieser Pfad** (`/pfad`, Ordner `/pfad/`, maskiert); je Pfad und Ebene eine Regel (wird ersetzt); entfernbar nur Regeln mit genau diesem Muster, globale nie; nach dem Speichern automatisch neu vergleichen, Filter und Zuklappen bleiben.
7. **macOS-App-Icon** mit transparentem Rand nach Apples Raster (824/1024); Web-Icons vollflächig.

## 6. Ziel v0.3: Übertragen „Direkt“ lokal

Umsetzungsreihenfolge Schritt 4 (ARCHITEKTUR Kap. 15): **Plan + Ausführung „Direkt“ lokal über den Worker-Pool, Journal, Protokoll.**

1. **Plan** (ARCHITEKTUR Kap. 7, Schritt 3): aus den angehakten Zeilen der Sitzung geordnete Operationen mit Abhängigkeiten – Ordner anlegen (auch leere) → Dateien kopieren/überschreiben → Rechte → Löschungen (tiefste Pfade zuerst). „Ersetzen durch“ kopiert den Inhalt der Ersatzdatei. `Bild.jpg` neu / `bild.jpg` löschen: **erst löschen, dann anlegen**.
2. **Bestätigung** vor dem Start: Zusammenfassung (anlegen/überschreiben/löschen, Bytes); **Löschungen rot und gesammelt einmal bestätigt** mit aufklappbarer Liste (E5, KONZEPT Kap. 4).
3. **Ausführung „Direkt“** in lokale Ziele über einen **Worker-Pool** (Threads, *n* = 1 ist nur ein Spezialfall – ARCHITEKTUR Kap. 8.1), Fortschritt über Channel (gedrosselt), Abbruch kooperativ, Fehler je Datei gesammelt (Lauf läuft weiter). **Nur auf das Ziel schreiben – die Quelle wird nie verändert (E3).**
4. **SQLite** (ARCHITEKTUR Kap. 4.2): Tabellen `lauf`, `journal`, `protokoll` (`manifest` und `stufen_status` erst mit FTPS). Jeder Schritt vorher im Journal angelegt, nachher aktualisiert. **Protokoll jeder Aktion (E8)** – auch Konfigurationsänderungen (Strecke speichern/löschen, „Regel setzen“).
5. **Oberfläche:** „Übertragen …“ freischalten; Bestätigungsdialog; Fortschritt; Ergebnis mit Fehlern je Datei; danach automatisch neu vergleichen (Erfolgskontrolle: alles „gleich“). Einfache Protokoll-Ansicht je Stufe.

**Mit Kim zu Beginn von v0.3 klären** (Vorschläge in Klammern, vorsichtige Variante):
- Datei erst als Temporärdatei im Zielordner schreiben, dann umbenennen? (ja – nie halbe Dateien am Ziel)
- Änderungszeit der Quelle am Ziel übernehmen? (ja – sonst wird beim nächsten Vergleich jede Datei per Hash geprüft)
- Rechte (`rechte` der Stufe) auch bei lokalen Zielen anwenden? (nur wenn gesetzt)
- Verhalten bei Fehlern: weiterlaufen (Standard) oder „bei Fehler stoppen“ als Schalter?
- Anzahl Worker lokal (Standard 8 wie in Kap. 8.1?)
- Ort der Datenbank (`~/Library/Application Support/de.sieber-bw.stellwerk/stellwerk.db`?) und Crate (`rusqlite` mit `bundled`?)
- Umfang der Protokoll-Ansicht in v0.3
- Abgebrochener „Direkt“-Lauf: laut KONZEPT 7.1 genügt neu vergleichen und neu übertragen – trotzdem beim Start einen Hinweis auf unvollständige Läufe zeigen?

Danach (v0.4 ff.): FTPS-Endpunkt mit Manifest und adaptiver Parallelität (Vergleich und Übertragung zum Server); dann Strategie „Umschalten“ mit Fortsetzen/Rollback.

## 7. Offene Punkte

- **Versionsnummer** steht noch auf 0.1.0 (`package.json`, `src-tauri/tauri.conf.json`, `Cargo.toml` → auch „Über Stellwerk“). Mit Kim klären, ob vor dem v0.2-Commit oder zu Beginn von v0.3 auf 0.2.0 bzw. 0.3.0 gehoben wird.
- Kim: echter Dateiname einer deutschen Dropbox-Konfliktkopie zur Prüfung der Erkennung.
- Lizenz (Open Source: MIT oder Apache-2.0?) und Open-Source-Lizenzliste im Über-Dialog (beim Build erzeugen).
- Webseite stellwerk.sieber-bw.de anlegen.
- Globale Standardregeln (`config.toml`) in der App bearbeitbar machen.
- Musterregeln für Dateirechte im Editor.
- „Ersetzen durch“, wenn die Zieldatei auf keiner Seite existiert.
- Hinweis, wenn eine Strecken-Regel wegen einer Stufen-Regel für denselben Pfad wirkungslos bleibt.
- Doku-Korrekturen: KONZEPT Kap. 5.3 nennt `Icon\r` (richtig: `Icon[\r]`); ARCHITEKTUR-Kopf verweist auf „E1–E9“ (KONZEPT hat E1–E11); ARCHITEKTUR Kap. 7 „Scan parallel“ (umgesetzt: nacheinander, s. v0.2-Doku Kap. 8).
- Typgenerierung mit `specta`/`tauri-specta` (20 Befehle).
- Alte Übergaben `docs/intern/UEBERGABE.md` (v0.1-Start) und `docs/intern/Übergabe für v0.2.md` sind durch dieses Dokument ersetzt.

## 8. Startprompt für den neuen Chat

> Wir arbeiten an meinem Projekt **Stellwerk** (`/Users/kim/Dropbox/Dev/GitHub/Stellwerk`). Lies bitte zuerst `docs/intern/Übergabe für v0.3.md`, dann `docs/v0.2 Entwicklungsdoku.md`, `docs/v0.1 Entwicklungsdoku.md`, `docs/KONZEPT.md` und `docs/ARCHITEKTUR.md`. Wir beginnen v0.3 mit Plan und Ausführung „Direkt“ für lokale Ziele (Kap. 6 der Übergabe) – kläre zuerst die offenen Fragen aus Kap. 6 mit mir. Du committest nie – das mache ich; du lieferst nach jedem Durchgang einen Commit-Text. Jede Funktion braucht einen automatischen Test. Terminal-Befehle für mich immer einzeln, Schritt für Schritt.
