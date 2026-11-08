# Übergabe – Stellwerk (Stand 06.11.2026)

Dieses Dokument dient als Einstieg in einen neuen Chat. Bitte zuerst lesen, dann die verlinkten Dokumente.

## 1. Projekt in einem Satz

**Stellwerk** ist eine Desktop-App (Tauri 2 · Rust · React/TypeScript) für kontrollierte Transportstrecken von Dateien: Quelle → (lokaler Spiegel zum Testen) → Server per FTPS, oder reine lokale Kopierstrecken – mit Vergleich nebeneinander, bewusster Auswahl, Regeln, Schlüsselbund und Protokoll.

## 2. Orte

| Was | Wo |
|---|---|
| Lokales Verzeichnis | `/Users/kim/Dropbox/Dev/GitHub/Stellwerk` |
| GitHub | https://github.com/KimSieber/Stellwerk (Branch `main`, initial gepusht) |
| App-Identifier | `de.sieber-bw.stellwerk` |
| Konfiguration (später) | `~/Library/Application Support/de.sieber-bw.stellwerk/` |

## 3. Arbeitsregeln

- **Claude committet niemals.** Ob ein Stand commit-reif ist und der Commit selbst bleiben immer bei Kim.
- Antworten auf Deutsch.
- Keine Automatismen bei Regeln/Ausnahmen – alles bewusst gesetzt.

## 4. Vorhandene Dateien

```
README.md                 Projektüberblick
.gitignore                (macOS-Icon-Muster bewusst als Icon[\r], NICHT Icon? – sonst wird src-tauri/icons ignoriert)
assets/icon/              stellwerk-icon.svg (Quelle), 1024er PNG, web/ (Favicons)
src-tauri/icons/          kompletter Tauri-Icon-Satz (icns, ico, PNGs)
src/                      leer (.gitkeep) – React folgt
docs/KONZEPT.md           Idee, Marktbetrachtung, Entscheidungen E1–E11, Regeln, Deploy-Modi, Detailentscheidungen
docs/ARCHITEKTUR.md       Architektur: Workspace, Datenmodell (TOML + SQLite), Endpunkt-Traits, Pipeline,
                          Ausführung inkl. adaptiver Parallelität, IPC, Frontend, Sicherheit, Tests, Reihenfolge
docs/intern/UEBERGABE.md  dieses Dokument
```

Icon: Weiche + grünes Signal auf dunkelblauem Verlauf (macOS-Squircle).

## 5. Wichtigste Entscheidungen (Kurzfassung – Details in KONZEPT/ARCHITEKTUR)

1. Nicht an Git gebunden; auch reine lokale Kopierstrecken (z. B. Dropbox → lokale KI-Laufzeitumgebung).
2. Strecke = Kette von Stufen; Endpunkte Lokal, FTPS (mind. explizites TLS), später SFTP.
3. **Einbahn:** nur Quelle → Ziel; Quelle wird nie verändert – im Code erzwungen (`SourceEndpoint` nur lesend).
4. Vorschau (Dry-Run) immer aktiv; Farben 🟢 neu · 🟡 geändert · 🔴 löschen · ⚪ geschützt/ignoriert, zeilengleich nebeneinander.
5. Löschungen: eine Sammelbestätigung, nicht einzeln.
6. **Stufen nie in einem Zug** – Zwischenstufe = Qualitäts-Check.
7. Regeln: Ignorieren · Schützen · Nur anlegen · Ersetzen durch; Ebenen Global → Strecke → Stufe; alles ab-/einschaltbar, Herkunft sichtbar; gitignore-Syntax.
8. Manifest (BLAKE3) **nur für entfernte Ziele**; lokal immer direkter Vergleich.
9. Deploy-Modi: Direkt oder Umschalten (Hochladen als `*.dpl-<lauf>` → Umbenennen mit `*.bak-<lauf>` → Übernehmen/Rollback), Journal in SQLite, fortsetzbar.
10. **Adaptive Parallelität:** möglichst viele Verbindungen (Start 8 bzw. gelernter Wert), bei Überlastung halbieren, langsam wieder hoch; gelernter Wert je Stufe. Worker-Pool von Anfang an.
11. Dateirechte standardmäßig nicht setzen; pro Stufe fest oder per Muster; 777 nur Warnung.
12. Pfade NFC-normalisiert; FTP binär; Groß/Klein-Konflikte warnen; Symlinks nicht folgen; Dropbox „nur online“ standardmäßig herunterladen (mit Volumenanzeige).
13. Passwörter nur im macOS-Schlüsselbund; TLS-Zertifikat geprüft, Ausnahme nur per gespeichertem Fingerprint.
14. Konfiguration als TOML (eine Datei je Strecke), Laufzeitdaten in SQLite.

## 6. Offene Punkte

- Lizenz (Open Source geplant: MIT oder Apache-2.0?)
- Sprache von Oberfläche und Doku (DE / EN / beides)
- Ggf. Icon an Mailwerk-Stil angleichen (Mailwerk-Icon lag nicht vor)

## 7. Nächster Schritt

Umsetzungsreihenfolge Schritt 1 (ARCHITEKTUR Kap. 15):
**Grundgerüst** – Cargo-Workspace, `crates/stellwerk-core` (Modell, TOML-Konfiguration laden/speichern/validieren), Tauri-2-Hülle + React/Vite/TypeScript-Frontend im bestehenden Ordner (vorhandene Icons in `src-tauri/icons` erhalten). Danach Schritt 2: Regel-Engine, Scan LocalFs, Diff – testgetrieben.

## 8. Startprompt für den neuen Chat

> Wir arbeiten an meinem Projekt **Stellwerk** (`/Users/kim/Dropbox/Dev/GitHub/Stellwerk`). Lies bitte zuerst `docs/intern/UEBERGABE.md`, dann `docs/KONZEPT.md` und `docs/ARCHITEKTUR.md`. Danach beginnen wir mit Schritt 1 der Umsetzungsreihenfolge (Grundgerüst). Du committest nie – das mache ich.
