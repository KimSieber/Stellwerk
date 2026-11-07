# Übergabe für v0.2 – Stellwerk (Stand 07.11.2026)

Einstieg für einen neuen Chat. Bitte zuerst dieses Dokument lesen, dann
[`docs/v0.1 Entwicklungsdoku.md`](../v0.1%20Entwicklungsdoku.md) (was gebaut ist),
[`docs/KONZEPT.md`](../KONZEPT.md) und [`docs/ARCHITEKTUR.md`](../ARCHITEKTUR.md).

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
- **Testpflicht:** Jede Funktion und jede Funktionsstrecke wird durch einen automatischen Test abgedeckt. `npm run pruefen` muss grün sein (Abdeckung 100 %, harte Grenze). Neue Tests mit **Gegenprobe** (Fehler absichtlich einbauen → Test wird rot).
- Antworten und Oberfläche auf **Deutsch**; Code-Bezeichner überwiegend deutsch (Strecke, Stufe, Eintrag …).
- **In kleinen Schritten:** höchstens zwei Arbeitspakete, dann Rückmeldung und Kims Test abwarten.
- Unklares vorher im Dialog klären; bei Konzeptlücken die vorsichtigere Variante wählen und **ausdrücklich zur Prüfung vorlegen**.
- **Keine Automatismen** bei Regeln/Ausnahmen – alles bewusst gesetzt.
- Kim prüft jede Änderung in der laufenden App (`npm run tauri dev`) und meldet Auffälligkeiten mit Screenshot.

### Technischer Ablauf mit Claude (bewährt in v0.1)

- Claude arbeitet in einer Linux-Umgebung, baut und testet dort (`cargo test`, `cargo llvm-cov`, Vitest, Playwright-Screenshots mit nachgebildeter Rust-Seite).
- Übertragung in Kims Ordner über die Claude-App: Archiv in den Ordner legen, mit `tar xzf … --overwrite` entpacken (Löschen ist im Ordner nicht erlaubt), danach Prüfsummen-Vergleich mit dem geprüften Stand. Das Archiv landet in `_to_delete/` – Kim löscht es.
- **`npm install` immer auf dem Mac** ausführen (Kim), nie in der Linux-Umgebung – sonst landen Linux-Programme in `node_modules`.
- macOS-spezifischer Code (`schluesselbund.rs`) wird zusätzlich gegen `aarch64-apple-darwin` geprüft (`cargo check --target …`).
- Ist der Mac nicht verbunden: Archiv als Datei im Chat bereitstellen; Kim lädt herunter und entpackt.

## 4. Stand v0.1 (abgeschlossen)

Commits: v0.1a Grundgerüst · v0.1b Regel-Engine, Scan · v0.1c Vergleich · v0.1d Editor, Schlüsselbund, „Über Stellwerk“ · Drag-and-drop, Ordnerprüfung · Standardregeln im Editor.

- **Bedienbar:** Strecken anlegen/bearbeiten/löschen, Stufen mit Quelle (lokal oder Vorstufe) und Ziel (lokal oder FTPS), Ordner per Dialog oder Drag-and-drop, FTPS-Passwort im Schlüsselbund, eigene Regeln je Strecke/Stufe, Standardregeln abschaltbar, Prüfung bei jeder Eingabe inkl. Existenz der Ordner, „Über Stellwerk“.
- **Im Kern fertig, noch ohne Oberfläche:** Scan lokaler Ordner, Vergleich Quelle ↔ Ziel (siehe Entwicklungsdoku Kap. 4.4/4.5).
- **Tests:** 132 Kern-, 16 Befehls-, 135 Frontend-Tests; Abdeckung 100 %.

## 5. Entscheidungen aus v0.1 (zusätzlich zu KONZEPT/ARCHITEKTUR)

1. **Einbahn in der Kette:** Eine Stufe schreibt nie in ihre eigene Quelle und nie in die Quelle einer früheren Stufe. Das Ziel einer Stufe darf Quelle einer späteren sein (Zweck der Kette).
2. **FTPS nur als Ziel** – nie vom Server herunterladen.
3. **Passwort:** Kim gibt nur das Passwort ein; Stellwerk vergibt den internen Schlüsselbund-Namen (`stellwerk:<strecke>:<stufe>`) selbst und zeigt ihn nicht an. Löschen einer Strecke entfernt ihre Passwörter.
4. **Neue Stufe:** Zielordner der Vorstufe wird als lokaler Quellpfad vorbelegt – nicht als Verweis „Ziel einer Vorstufe“ (irritierte, provozierte Fehler).
5. **Lokale Ordner müssen existieren** – geprüft live und verbindlich vor dem Speichern.
6. **Meldungen** direkt unter dem betroffenen Feld (rot); Knöpfe und Speicher-Meldungen bleiben beim Scrollen sichtbar.
7. **Vergleich (vorsichtige Auslegung, von Kim noch nicht ausdrücklich bestätigt):**
   - Schützen = am Ziel nie anlegen, überschreiben oder löschen (auch Neues nicht).
   - Nur anlegen = nur, wenn am Ziel fehlend; sonst unangetastet, nie löschen.
   - Ersetzen durch ohne Ersatzdatei = Konflikt.
   - Ordner nur am Ziel mit bleibendem Inhalt werden nicht gelöscht, sondern geschützt.
   - Nicht vorausgewählt: Konflikte samt Inhalt, Dropbox-Konfliktkopien, „nur online“ bei Einstellung „überspringen“.
   - Symlinks sind immer Konflikt.
   - `Bild.jpg`/`bild.jpg`: Hinweis; beim Planen erst löschen, dann anlegen.
8. **Ignorierte Einträge** werden grau angezeigt, ignorierte Ordner aber nicht durchlaufen.
9. Globale Standardregel für das macOS-Ordnersymbol: `Icon[\r]`.

## 6. Ziel v0.2: Vergleichsansicht

Umsetzungsreihenfolge Schritt 3 (ARCHITEKTUR Kap. 15), Rest:

1. **Tauri-Befehle** für den Vergleich: `vergleich_starten(strecke, stufe)` (Scan beider Seiten + Diff, Ergebnis im Rust-State, liefert `vergleich_id` und Zusammenfassung), `diff_seite(vergleich_id, offset, limit, filter)`, `auswahl_setzen(vergleich_id, pfade, an)`. Fortschritt über Tauri-Channels, gedrosselt. Abbruch.
   - Ordner-Stufen mit Quelle „Ziel einer Vorstufe“ über `effektive_quelle` auflösen.
   - FTPS-Ziele: in v0.2 noch nicht vergleichbar → klare Meldung.
2. **Vergleichsansicht** (KONZEPT Kap. 4): Quelle und Ziel nebeneinander, zeilengleich; Farben 🟢 neu · 🟡 geändert · 🔴 löschen · ⚪ geschützt/ignoriert; Häkchen je Zeile; Filter (nur Änderungen / alle / Warnungen); Baum ein-/ausklappbar; virtualisierte Liste (`@tanstack/react-virtual`); Zusammenfassung mit Bytes und „nur online“; Warnungen sichtbar; Herkunft der Regel als Tooltip.
3. Von der Startseite bzw. dem Editor eine Stufe **öffnen** (nie ausführen – E10).
4. Kontextmenü „Regel setzen“ aus der Vergleichsansicht (kann auch v0.3 werden – mit Kim klären).

Danach (v0.3 ff.): Plan + Ausführung „Direkt“ lokal über Worker-Pool, Journal und Protokoll in SQLite; dann FTPS mit Manifest und adaptiver Parallelität; dann „Umschalten“ mit Rollback.

## 7. Offene Punkte

- Kim: Bestätigung der Vergleichs-Auslegung (Kap. 5, Punkt 7).
- Kim: echter Dateiname einer deutschen Dropbox-Konfliktkopie zur Prüfung der Erkennung.
- Lizenz (Open Source: MIT oder Apache-2.0?).
- Open-Source-Lizenzliste im Über-Dialog (beim Build erzeugen).
- Webseite stellwerk.sieber-bw.de anlegen.
- Globale Standardregeln (`config.toml`) in der App bearbeitbar machen.
- Musterregeln für Dateirechte im Editor.
- „Ersetzen durch“, wenn die Zieldatei auf keiner Seite existiert.
- Doku-Korrekturen: KONZEPT Kap. 5.3 nennt `Icon\r` (richtig: `Icon[\r]`); ARCHITEKTUR-Kopf verweist auf „E1–E9“ (KONZEPT hat E1–E11).
- Typgenerierung mit `specta`/`tauri-specta`, sobald die Befehlszahl wächst.
- Alte Übergabe `docs/intern/UEBERGABE.md` (Stand v0.1-Start) ist durch dieses Dokument ersetzt.

## 8. Startprompt für den neuen Chat

> Wir arbeiten an meinem Projekt **Stellwerk** (`/Users/kim/Dropbox/Dev/GitHub/Stellwerk`). Lies bitte zuerst `docs/intern/Übergabe für v0.2.md`, dann `docs/v0.1 Entwicklungsdoku.md`, `docs/KONZEPT.md` und `docs/ARCHITEKTUR.md`. Wir beginnen v0.2 mit der Vergleichsansicht (Kap. 6 der Übergabe). Du committest nie – das mache ich; du lieferst nach jedem Durchgang einen Commit-Text. Jede Funktion braucht einen automatischen Test.
