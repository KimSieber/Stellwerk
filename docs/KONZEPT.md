# Stellwerk – Konzept, Umsetzungsweg und Entscheidungen

Stand: 06.11.2026 · Status: Konzeptphase
 
---

## 1. Idee und Ausgangslage

Bisheriger Ablauf beim Veröffentlichen von Webseiten, APIs u. ä.:

1. Entwicklung in einem Git-Verzeichnis auf dem Mac, Push nach GitHub.
2. Ist ein Stand erreicht, wird er in ein **lokales Verzeichnis kopiert, das den Server spiegelt**. Dort wird getestet.
3. Anschließend wird der Stand per FTP auf den Server hochgeladen.

Vorhandene FTP-Werkzeuge decken nur den Transfer ab, nicht die Strecke als Ganzes.
**Stellwerk** unterstützt die komplette Strecke: vergleichen, bewusst auswählen, freigeben, übertragen, protokollieren.

Darüber hinaus soll das Werkzeug **allgemein für Kopierstrecken** taugen, z. B. „PictureSearchView von der Dropbox in die lokale KI-Laufzeitumgebung“.

### Name

*Stellwerk*: Im Stellwerk werden Strecken überwacht, Weichen gestellt und Züge erst freigegeben, wenn alles stimmt. Die Begriffe des Tools (Strecke, Stufe, Umschalten, Signal/Freigabe) folgen diesem Bild. Passt zur Namensfamilie (Mailwerk).

### Icon

Weiche (Gleis teilt sich) + grünes Signal (Freigabe). Quelle: `assets/icon/stellwerk-icon.svg`.

---

## 2. Marktbetrachtung (Kurzfassung)

| Werkzeug | Stärke | Warum nicht ausreichend |
|---|---|---|
| FreeFileSync | Vergleich, Filter, FTP/SFTP, gespeicherte Jobs | Bedienung sperrig, kein Schlüsselbund, keine Strecken mit mehreren Stufen |
| git-ftp | Lädt Git-Diffs hoch, merkt sich Stand | CLI, an Git gebunden, kein Zwischenschritt |
| FTP-Deploy-Action (GitHub) | Automatischer Deploy | keine manuelle Auswahl, an GitHub/Git gebunden |
| rsync/SSH | Profi-Standard | braucht SSH-Zugang, keine Oberfläche |
| Transmit, Beyond Compare, ForkLift | gute Mac-Clients | kostenpflichtig, Transfer-Fokus |

**Fazit:** Teile existieren, die Kombination (Strecke mit mehreren Stufen, visueller Vergleich, bewusste dauerhafte Regeln, Schlüsselbund, Protokoll) nicht.

---

## 3. Grundprinzipien (Entscheidungen)

| # | Entscheidung | Begründung |
|---|---|---|
| E1 | **Nicht an Git gebunden.** Keine CI/GitHub-Actions-Pflicht. | Flexibilität; auch für reine Kopierstrecken nutzbar. |
| E2 | **Strecke = Kette von Stufen**; jede Stufe verbindet zwei Endpunkte (Lokal, FTPS, später SFTP). | Ein Modell für alle Fälle; Stufe 1 und 2 nutzen denselben Code. |
| E3 | **Einbahn-Prinzip:** Übertragung nur Quelle → Ziel. Auf die Quelle wird **nie** geschrieben oder gelöscht. Fehlt etwas auf dem Ziel, wird es dort angelegt – nie wird deshalb die Quelle verändert. | Schutz der Entwicklungsumgebung. Technisch abgesichert: Quell-Endpunkt hat nur eine Lese-Schnittstelle. |
| E4 | **Vorschau (Dry-Run) ist immer aktiv.** Nichts passiert ohne Knopfdruck. | Kontrolle. |
| E5 | **Löschungen** werden rot markiert und **gesammelt** einmal bestätigt (nicht einzeln). | Sicherheit ohne Klick-Marathon. |
| E6 | **FTP heißt mindestens explizites TLS (FTPS).** Klartext-FTP wird nicht angeboten. SFTP folgt später. | Zugangsdaten und Inhalte verschlüsselt. |
| E7 | **Zugangsdaten im macOS-Schlüsselbund**, nie in Konfigurationsdateien. | Sicherheit. |
| E8 | **Jede Aktion wird protokolliert** (SQLite). | Nachvollziehbarkeit. |
| E9 | **Ausnahmen/Unterschiede nur bewusst gesetzt**, kein Automatismus, getrennt pro Stufe. | Volle Flexibilität, z. B. Server-Konfiguration bewusst im Spiegel. |

---

## 4. Vergleichsansicht

- Quelle und Ziel **nebeneinander, zeilengleich**.
- Farben: 🟢 **neu** (nur in Quelle) · 🟡 **geändert** · 🔴 **wird gelöscht** (nur im Ziel; Quellseite bleibt leer) · ⚪ grau = geschützt/ignoriert (sichtbar, aber unangetastet).
- Pro Datei/Ordner: Häkchen zum Ein-/Ausschließen für diesen Lauf.
- Per Kontextmenü: dauerhafte Regel setzen (siehe Kap. 5).
- Bestätigungsdialog bei Löschungen: Summe („37 Dateien / 4 Ordner“), aufklappbare Liste.

---

## 5. Regeln

### 5.1 Regeltypen

| Regel | Wirkung | Beispiel |
|---|---|---|
| **Ignorieren** | weder verglichen noch übertragen | `.DS_Store`, `node_modules/` |
| **Schützen** | angezeigt (grau), am Ziel nie überschrieben/gelöscht | Server-Logs, `config.php` im Spiegel |
| **Nur anlegen** | nur übertragen, wenn am Ziel fehlend | Start-Konfiguration |
| **Ersetzen durch** (optional) | am Ziel landet eine andere Quelldatei | `config.server.php` → `config.php` |

Syntax der Muster: `.gitignore`-Format.

### 5.2 Ebenen

1. **Global** – Standardvorgaben (Vorlage)
2. **Strecke** – Vorgaben ab-/einschaltbar, eigene Regeln ergänzbar
3. **Stufe** – dasselbe nochmals je Teilstrecke

Jede Regel zeigt in der Regelübersicht ihre **Herkunft** und ist per Schalter an/aus.

### 5.3 Globale Standard-Ausschlüsse (je Strecke/Stufe abschaltbar)

`.DS_Store` (praktisch immer), `._*`, `Icon\r`, `Thumbs.db`, `desktop.ini`, `.git/`, `.svn/`, `.idea/`, `.vscode/`, `*.swp`, `*~`, `.dropbox`, `.dropbox.cache/`.
Dropbox-Konfliktkopien werden nicht übertragen, sondern als **Warnung** angezeigt.

---

## 6. Vergleichsverfahren & Manifest

- FTP liefert keine verlässlichen Zeitstempel und keine Hashes.
- Deshalb führt Stellwerk pro Stufe ein **Manifest** (Pfad, Größe, Hash [BLAKE3], Zeitpunkt) des zuletzt übertragenen Stands.
- Vergleich Quelle ↔ Manifest statt Quelle ↔ Server-Zeitstempel. **Vollabgleich** (Server neu einlesen) als langsamer Kontroll-/Notfallmodus.
- Lokal ↔ Lokal: direkter Vergleich (Größe, mtime, bei Bedarf Hash).

---

## 7. Deployment-Modi

Pro Strecke/Stufe wählbar:

### 7.1 Direkt
Dateien werden direkt überschrieben. Bei Abbruch: Vergleich und Upload neu starten. Für kleine Änderungen.

### 7.2 Mit Umschalten (unterbrechungsarm)

**Phase 1 – Hochladen** (langsam, unkritisch)
Geänderte/neue Dateien als Schattendatei mit **Suffix**: `index.php.dpl-<id>`. Suffix statt Präfix, damit der Server keine halbfertigen Skripte ausführt; `.htaccess` kann `*.dpl-*` sperren. Live-Seite unverändert. Pausieren/Abbrechen jederzeit; Abbruch entfernt nur Schattendateien.

**Phase 2 – Umschalten** (schnell, nur auf Knopfdruck)
Original → `name.bak-<id>`, Schattendatei → Original. Löschungen ebenfalls als Umbenennung nach `.bak-<id>`. Nur Umbenennungen → Sekunden statt Minuten. (Atomar ist es über FTP nicht, aber das Zeitfenster schrumpft drastisch.) Optional: Wartungsseite während Phase 2.

**Phase 3 – Übernehmen oder Rollback**
Übernehmen: `.bak-*` löschen. Rollback: `.bak-*` zurückbenennen.

**Transparenz:** Deploy-Journal (SQLite) mit Status pro Datei (*ausstehend → hochgeladen → umgeschaltet → bereinigt*). UI zeigt Phase, Fortschritt, offene Dateien. Fortsetzen nach Abbruch anhand Journal + Server-Listing. Verwaiste `*.dpl-*`/`*.bak-*` werden erkannt und zum Aufräumen angeboten.

---

## 8. Technische Detailentscheidungen

| Thema | Entscheidung |
|---|---|
| Übertragungsmodus | FTP immer **binär** (`TYPE I`); Zeilenenden werden nie verändert. |
| Unicode | Pfade vor Vergleich nach **NFC normalisieren** (macOS NFD vs. Linux NFC – Umlaute!). |
| Groß-/Kleinschreibung | Konflikte (`Bild.jpg`/`bild.jpg`) erkennen und warnen; reine Schreibweise-Umbenennung auf dem Server als Löschen + Hochladen. |
| Dateirechte | Standard: **nicht setzen**. Pro Stufe einstellbar: feste Werte (z. B. 755/644) oder Regeln nach Muster (`*.sh` → 755). Bei `777` Warnung, kein Verbot. `SITE CHMOD`-Unterstützung wird beim Verbinden geprüft; Fehler werden deutlich protokolliert. |
| Symlinks | nicht folgen, Hinweis anzeigen; Folgen per Einstellung. |
| Lokale Kopien | mtime übernehmen; leere Ordner mitübertragen. |
| Dropbox „nur online“ | Standard: **herunterladen**, vorher Anzahl/Volumen in der Vorschau anzeigen. Optional pro Stufe: überspringen + warnen. |
| Große Dateien | Fortschritt pro Datei, Wiederaufnahme per `REST`. |
| Performance | Manifest statt rekursivem `LIST`, Caching, parallele Verbindungen. |

---

## 9. Architektur & Stack

- **Tauri 2** – Desktop-App (primär macOS, Windows möglich)
- **Rust** – gesamte Logik (Scan, Vergleich, Transfer, Journal); Frontend zeigt nur an
- **React + TypeScript** – Oberfläche
- **SQLite** (`rusqlite`) – Protokoll, Deploy-Journal, Manifeste
- Konfiguration der Strecken: JSON/TOML

Geplante Crates: `suppaftp` (FTPS), später `russh`/`ssh2` (SFTP), `keyring` (Schlüsselbund), `walkdir` + `ignore` (Dateibaum, gitignore-Syntax), `blake3` (Hashes), `unicode-normalization`.

**Kernabstraktion:** Trait `Endpoint` mit Implementierungen `LocalFs`, `Ftps`, `Sftp`. Quell-Rolle erhält nur `ReadEndpoint` (E3).

---

## 10. Umsetzungsweg

**MVP**
1. Strecken anlegen (Stufen, Endpunkte, Regeln im gitignore-Format)
2. Vergleichsansicht nebeneinander mit Auswahl
3. Lokale Stufe (Lokal → Lokal) inkl. Vorschau – ohne Netzwerk sauber testbar
4. FTPS-Stufe mit Manifest, Sammelbestätigung bei Löschungen, Schlüsselbund
5. Protokoll

**Danach**
- Deployment-Modus „Mit Umschalten“ inkl. Journal und Rollback
- SFTP
- Backup vor Deploy
- optional: Git-Commit/Tag des deployten Stands im Manifest vermerken (rein informativ, keine Git-Pflicht)

---

## 11. Offene Punkte

- Lizenz (Open Source: MIT / Apache-2.0?)
- Sprache der Oberfläche/Doku (DE, EN, beides)
- Datenmodell im Detail (Strecke, Stufe, Endpunkt, Regel, Manifest, Journal)
- UI-Entwurf (Startseite Strecken, Vergleichsansicht, Phasen-Ablauf)
