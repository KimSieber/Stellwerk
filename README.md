<p align="center">
  <img src="assets/icon/stellwerk-icon-1024.png" width="160" alt="Stellwerk Icon">
</p>

<h1 align="center">Stellwerk</h1>
<p align="center"><b>Kontrollierte Transportstrecken für Dateien – lokal, FTPS und (später) SFTP.</b></p>

---

> **Status:** v0.1 – Strecken erfassen und prüfen (Editor, Schlüsselbund, Regeln); Vergleich im Kern. Noch keine Übertragung.

## Worum geht es?

Stellwerk bringt Dateien kontrolliert von A nach B – und optional weiter nach C.
Typischer Fall: Entwicklung im Arbeitsverzeichnis → lokaler Server-Spiegel zum Testen → Upload auf den Webserver per FTPS.
Ebenso: reine lokale Kopierstrecken, z. B. von der Dropbox in eine lokale Laufzeitumgebung.

Anders als klassische FTP-Clients steht nicht der Transfer im Mittelpunkt, sondern die **Strecke**:
vergleichen → bewusst auswählen → freigeben → übertragen → protokollieren.

## Kernfunktionen (geplant)

- **Strecken** mit einer oder mehreren **Stufen** (Lokal → Lokal → FTPS …)
- **Vergleich nebeneinander**, zeilengleich: 🟢 neu · 🟡 geändert · 🔴 wird gelöscht
- Dateien per Auswahl ein-/ausschließen; Vorschau (Dry-Run) ist immer aktiv
- **Einbahn-Prinzip:** Quelle wird nie verändert, Löschungen nur auf dem Ziel (Sammelbestätigung)
- **Regeln** pro Stufe: Ignorieren · Schützen · Nur anlegen · Ersetzen durch – mit sichtbarer Herkunft (global / Strecke / Stufe)
- **Unterbrechungsarmes Deployment** in drei Phasen (Hochladen → Umschalten → Aufräumen/Rollback)
- Zugangsdaten im **macOS-Schlüsselbund**, Verbindungen mindestens mit **explizitem TLS**
- **Protokoll** jeder Aktion

## Technik

Tauri 2 · Rust (Logik) · React + TypeScript (Oberfläche) · SQLite (Journal, Protokoll)

## Entwicklung

Voraussetzungen: Rust (stable), Node.js ≥ 20, Xcode Command Line Tools.

```bash
npm install
npm run tauri dev                 # App im Entwicklungsmodus
cargo test -p stellwerk-core      # Tests des Kerns
```

Konfiguration: `~/Library/Application Support/de.sieber-bw.stellwerk/` (`config.toml`, `strecken/<id>.toml`).

## Dokumentation

- [Konzept & Entscheidungen](docs/KONZEPT.md)
- [Architektur](docs/ARCHITEKTUR.md)
- [v0.1 Entwicklungsdoku](docs/v0.1%20Entwicklungsdoku.md)
- Interne Notizen: `docs/intern/`

## Lizenz

Noch offen (Veröffentlichung als Open Source geplant).
