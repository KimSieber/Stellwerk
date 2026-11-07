# Übergabe für v0.4 – Stellwerk (Stand 07.11.2026)

Einstieg für einen neuen Chat. Bitte zuerst dieses Dokument lesen, dann
[`docs/v0.3 Entwicklungsdoku.md`](../v0.3%20Entwicklungsdoku.md) (was in v0.3 dazukam),
[`docs/v0.2 Entwicklungsdoku.md`](../v0.2%20Entwicklungsdoku.md), [`docs/v0.1 Entwicklungsdoku.md`](../v0.1%20Entwicklungsdoku.md),
[`docs/KONZEPT.md`](../KONZEPT.md) und [`docs/ARCHITEKTUR.md`](../ARCHITEKTUR.md) – für v0.4 besonders KONZEPT Kap. 6 und 8, ARCHITEKTUR Kap. 4.2 (`manifest`, `stufen_status`), 5, 8.1 und 11.

## 1. Projekt in einem Satz

**Stellwerk** ist eine Desktop-App (Tauri 2 · Rust · React/TypeScript) für kontrollierte Transportstrecken von Dateien: Quelle → lokaler Spiegel (Test) → Server per FTPS, oder reine lokale Kopierstrecken – mit Vergleich nebeneinander, bewusster Auswahl, Regeln, Schlüsselbund und Protokoll.

## 2. Orte

| Was | Wo |
|---|---|
| Lokales Verzeichnis | `/Users/kim/Dropbox/Dev/GitHub/Stellwerk` |
| GitHub | https://github.com/KimSieber/Stellwerk (Branch `main`) |
| App-Identifier | `de.sieber-bw.stellwerk` |
| Konfiguration und Laufzeitdaten | `~/Library/Application Support/de.sieber-bw.stellwerk/` (`config.toml`, `strecken/`, `stellwerk.sqlite`) |
| Webseite (geplant, noch nicht angelegt) | https://stellwerk.sieber-bw.de |
| Vorbild Drag-and-drop | PaperTrail Compare, `src/hooks/useDragDropTarget.ts` |
| Vorbild „Über …“ | Mailwerk, `Mailwerk/Views/About/AboutView.swift`, `Services/About/AppInfo.swift` |

## 3. Arbeitsregeln (verbindlich)

- **Claude committet niemals.** Ob ein Stand commit-reif ist und der Commit selbst bleiben immer bei Kim. Claude liefert nach jedem Durchgang einen **Commit-Text**. Kim committet nur Stände mit sichtbarer Veränderung – mehrere Durchgänge können einen Commit-Text teilen.
- **Testpflicht:** Jede Funktion und jede Funktionsstrecke wird durch einen automatischen Test abgedeckt. `npm run pruefen` muss grün sein (Abdeckung 100 % Funktionen/Zeilen, Frontend auch Zweige – harte Grenze). Neue Tests mit **Gegenprobe** (Fehler absichtlich einbauen → Test wird rot).
- Antworten und Oberfläche auf **Deutsch**; Code-Bezeichner überwiegend deutsch.
- **In kleinen Schritten:** höchstens zwei Arbeitspakete, dann Rückmeldung und Kims Test abwarten.
- Unklares vorher im Dialog klären; bei Konzeptlücken die vorsichtigere Variante wählen und **ausdrücklich zur Prüfung vorlegen**.
- **Keine Automatismen** bei Regeln/Ausnahmen – alles bewusst gesetzt.
- Kim prüft jede Änderung in der laufenden App (`npm run tauri dev`) und meldet Auffälligkeiten mit Screenshot.
- **Terminal-Befehle für Kim nie als Block:** ein Befehl pro Schritt, Ausgabe abwarten, dann der nächste. Keine `#`-Kommentare in Befehlszeilen (zsh führt sie als Befehl aus).

### Technischer Ablauf mit Claude (bewährt in v0.1–v0.3)

- Claude arbeitet in einer Linux-Umgebung, baut und testet dort: `cargo test`, `cargo llvm-cov`, Vitest, Playwright-Aufnahmen einer Vorschau-Seite mit `mockIPC`. Für Tauri unter Linux einmalig die WebKitGTK-Pakete installieren (`libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libssl-dev`), `cargo-llvm-cov` und `llvm-tools-preview`.
- **Stand holen:** In v0.3 war das Klonen des GitHub-Repos nicht erlaubt. Bewährt hat sich, Kims Ordner in der Claude-App freizugeben und die Dateien **direkt aus Kims Ordner bereitzustellen** (bis 50 je Aufruf, ohne `node_modules`, `target`, `dist`, `.git`, `assets`, `docs/intern`). Kein Archiv – in der Geräte-Shell gibt es keinen Ablageordner, aus dem sich ein Archiv bereitstellen ließe. Eine unveränderte Kopie als Basis aufheben (für den Abgleich vor dem Zurückschreiben).
- **Vor dem Zurückschreiben** prüfen, ob Kim inzwischen etwas geändert hat (Prüfsummen bzw. `find -newer`). npm berührt `package-lock.json` gelegentlich ohne Inhaltsänderung – dann Prüfsumme vergleichen.
- **Übertragung** in Kims Ordner: geänderte Dateien **einzeln an ihren Platz** schreiben (bis 50 je Übertragung), danach Prüfsummen-Vergleich. Keine Archive, kein Ordner `_to_delete/`. Mehr als 50 Dateien: vorher melden.
  - Für jede Übertragung einen **neuen** Bereitstellungsordner verwenden.
  - PNG/SVG bekommen bei der Übertragung Herkunfts-Metadaten (C2PA) – Prüfsummen weichen ab, Bilddaten sind gleich.
- **`npm install` immer auf dem Mac** (Kim), nie in der Linux-Umgebung. In der Linux-Umgebung `npm ci --ignore-scripts` nur in der eigenen Arbeitskopie.
- **`package.json` und `package-lock.json` nie blind überschreiben:** Kims npm (Version 12) setzt Markierungen anders, und `package.json` enthält `"allowScripts": { "fsevents": false }`. Nur die eigene Änderung einarbeiten.
- macOS-spezifischer Code (`schluesselbund.rs`) zusätzlich gegen `aarch64-apple-darwin` prüfen.
- **Dateisystem-Unterschiede bedenken:** macOS (APFS) unterscheidet Groß/Klein nicht und speichert Umlaute oft zerlegt (NFD), Linux nicht. Tests, die Namen prüfen, lesen die **tatsächlichen Namen** (`read_dir`) statt `exists()`. In v0.3 lief ein Test nur auf dem Mac rot.
- **Kein `git` über die Geräte-Shell** auswerten (Linux-VM; NFD-Dateinamen erscheinen dort als unversioniert).

## 4. Stand v0.3 (abgeschlossen)

- **Bedienbar:** Startseite → Klick auf die Strecke → **Streckenansicht**: alle Stufen im Hintergrund verglichen, je Stufe Status (was zu tun ist) und Protokoll der Läufe bis zu jedem Schritt. „Ansehen & übertragen …“ → Vergleichsansicht mit **„Übertragen …“**: Plan, Sammelbestätigung der Löschungen, „Beim ersten Fehler anhalten“, Fortschritt mit Abbrechen, Ergebnis mit Fehlern je Datei, danach automatisch neu verglichen (und in der Streckenansicht die folgende Stufe). Version 0.3.0.
- **Kern:** `plan` (Operationen mit Abhängigkeiten), `exec` (Worker-Pool `min(8, Kerne)`, Temporärdatei + Umbenennen, Änderungszeit, Abbruch), `store` (SQLite `lauf`, `journal`, `protokoll`).
- **Protokoll (E8):** nur Aktionen auf Laufwerken/Servern – Läufe (Start, Fehler je Operation, Ende) und jeder Schritt im Journal. Einstellungen in Stellwerk werden nicht protokolliert.
- **Noch nicht möglich:** FTPS-Stufen zeigen in der Streckenansicht „Vergleich folgt mit dem Manifest (v0.4)“; „Mit Umschalten“ wird im Dialog abgewiesen.
- **Tests:** 234 Kern-, 43 Befehls-, 245 Frontend-Tests; Abdeckung 100 %.

## 5. Entscheidungen aus v0.3 (zusätzlich zu KONZEPT/ARCHITEKTUR)

1. **Temporärdatei immer** im Modus „Direkt“ (`.<name>.stw-<lauf>`, dann umbenennen) – kein eigener Schalter. „Mit Umschalten“ bleibt die je Stufe wählbare große Lösung.
2. **Änderungszeit der Quelle** wird am Ziel übernommen – bei FTPS nur, wenn der Server es kann (`MFMT`), sonst über das Manifest.
3. **Rechte** nur, wenn bei der Stufe gesetzt. Ohne Einstellung: neue Dateien wie die Quelle, überschriebene behalten ihre Rechte.
4. **„Bei Fehler stoppen“** als Schalter im Dialog; Standard: weiterlaufen.
5. **Worker:** so viele wie sicher betreibbar – lokal `min(8, Prozessorkerne)`.
6. **Datenbank** `stellwerk.sqlite` im App-Ordner (`rusqlite`, bundled).
7. **Nachvollziehbarkeit:** Jeder Lauf und jede Veränderung **auf Laufwerken/Servern** muss nachvollziehbar sein (Journal je Operation, Protokoll). Einstellungen in Stellwerk nicht („Ich muss keine Einstellungen rechtfertigen, nur den Ablauf von Ergebnissen“) – KONZEPT E8 präzisiert.
8. **Kein Hinweis beim Start** auf abgebrochene „Direkt“-Läufe.
9. **Ein Plan gilt genau einmal;** jede Auswahländerung verwirft ihn. Löschungen ohne Bestätigung → nichts wird ausgeführt.
10. **Zweite Übertragung derselben Stufe** wird abgewiesen (nicht wie beim Vergleich der alte abgebrochen).
11. **Überblick über eine Strecke** (Kims Entwurf): Startseite ohne Status und ohne „Vergleichen“ – Klick auf die Kachel öffnet die Streckenansicht; dort werden alle Stufen verglichen, je Stufe Status links, Protokoll rechts (neueste oben). Kein Vergleich aller Strecken beim Start (mit FTPS zu aufwendig).
12. **Status zählt nur, was zu tun ist** (neu, geändert, löschen, Konflikt); Geschütztes/Ignoriertes ist gewollt.

## 6. Noch offen aus v0.3 – zu Beginn mit Kim klären

- **Neu vergleichen, wenn Stellwerk wieder in den Vordergrund kommt?** Kim: „Wenn ich weiterentwickle, soll ich im Stellwerk sehen, dass eine neue Änderung vorliegt.“ Bisher wird beim Öffnen der Strecke, nach einer Übertragung (folgende Stufe) und über „Alle neu vergleichen“ verglichen. Vorschlag: zusätzlich bei Rückkehr ins Fenster, wenn die Streckenansicht offen ist und der letzte Vergleich älter als z. B. 1 Minute ist (lokale Stufen; FTPS nur gegen das Manifest).

## 7. Ziel v0.4: FTPS

Umsetzungsreihenfolge Schritt 5 (ARCHITEKTUR Kap. 15): **Schlüsselbund + FTPS-Endpunkt + Manifest + adaptive Parallelität.**

1. **Endpunkte als Traits** (ARCHITEKTUR Kap. 5): Quelle nur lesend, Ziel schreibend (A4). `exec` arbeitet dann gegen den Ziel-Endpunkt statt direkt mit `std::fs`; LocalFs wird der erste Endpunkt (Tests von v0.3 bleiben gültig).
2. **FTPS-Endpunkt:** explizites TLS Pflicht (E6), immer `TYPE I`, `MLSD` bevorzugt (Fallback `LIST`), Passwort aus dem Schlüsselbund (E7), TLS-Zertifikat geprüft, Ausnahme nur per gespeichertem Fingerprint (ARCHITEKTUR Kap. 11). Verbindungstest im Editor (`verbindung_testen`, Fähigkeiten: chmod, MFMT, MLSD).
3. **Manifest** (KONZEPT Kap. 6, E11): Tabelle `manifest`; Vergleich Quelle ↔ Manifest; „Vollabgleich“ liest den Server. Nach erfolgreichem Lauf Manifest aktualisieren.
4. **Adaptive Parallelität** (ARCHITEKTUR Kap. 8.1): Verbindungspool, Start 8 bzw. gelernter Wert, bei Überlastung halbieren, langsam hoch; gelernter Wert in `stufen_status`, sichtbar in der Oberfläche.
5. Vergleich und „Übertragen …“ für FTPS-Stufen freischalten – in der Streckenansicht reihen sie sich in den Vergleich beim Öffnen ein (gegen das Manifest, ohne Serverkontakt).

**Mit Kim zu Beginn von v0.4 klären** (Vorschläge in Klammern, vorsichtige Variante):
- Crate für FTPS (`suppaftp` mit `rustls` wie in ARCHITEKTUR Kap. 5?) und ob der Kern dafür eine async-Laufzeit (`tokio`) bekommt oder synchron mit Threads bleibt.
- Testserver: Docker-Container mit FTPS (ARCHITEKTUR Kap. 13) – läuft das auf Kims Mac (OrbStack scheint vorhanden) und in der Linux-Umgebung?
- Temporärdatei auch bei FTPS (`.<name>.stw-<lauf>` hochladen, dann `RNFR/RNTO`)? (ja – nie halbe Dateien am Server)
- Erster Vergleich ohne Manifest: Server vollständig lesen und Manifest daraus anlegen, Inhalte per Größe/Zeit oder herunterladen und hashen? (Größe + Hinweis; Hash nur auf Wunsch)
- Umgang mit Servern ohne `MFMT` (Zeit nicht setzbar) – Manifest genügt?

Danach (v0.5 ff.): Strategie „Umschalten“ mit Fortsetzen/Rollback.

## 8. Offene Punkte (allgemein)

- Kim: echter Dateiname einer deutschen Dropbox-Konfliktkopie zur Prüfung der Erkennung.
- Lizenz (Open Source: MIT oder Apache-2.0?) und Open-Source-Lizenzliste im Über-Dialog (beim Build erzeugen).
- Webseite stellwerk.sieber-bw.de anlegen.
- Globale Standardregeln (`config.toml`) in der App bearbeitbar machen.
- Musterregeln für Dateirechte im Editor.
- „Ersetzen durch“, wenn die Zieldatei auf keiner Seite existiert.
- Hinweis, wenn eine Strecken-Regel wegen einer Stufen-Regel für denselben Pfad wirkungslos bleibt.
- Typgenerierung mit `specta`/`tauri-specta` (jetzt 25 Befehle).
- Die alten Übergaben `docs/intern/UEBERGABE.md`, `Übergabe für v0.2.md` und `Übergabe für v0.3.md` sind durch dieses Dokument ersetzt (Kim entscheidet, ob sie bleiben).

## 9. Startprompt für den neuen Chat

> Wir arbeiten an meinem Projekt **Stellwerk** (`/Users/kim/Dropbox/Dev/GitHub/Stellwerk`). Lies bitte zuerst `docs/intern/Übergabe für v0.4.md`, dann `docs/v0.3 Entwicklungsdoku.md`, `docs/v0.2 Entwicklungsdoku.md`, `docs/v0.1 Entwicklungsdoku.md`, `docs/KONZEPT.md` und `docs/ARCHITEKTUR.md`. Kläre zuerst die offenen Fragen aus Kap. 6 und 7 der Übergabe mit mir, dann beginnen wir v0.4 (FTPS). Du committest nie – das mache ich; du lieferst nach jedem Durchgang einen Commit-Text. Jede Funktion braucht einen automatischen Test. Terminal-Befehle für mich immer einzeln, Schritt für Schritt.
