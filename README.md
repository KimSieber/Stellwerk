<p align="center">
  <img src="assets/icon/stellwerk-icon-1024.png" width="160" alt="Stellwerk Icon">
</p>

<h1 align="center">Stellwerk</h1>
<p align="center"><b>Kontrollierte Transportstrecken für Dateien – lokal, FTPS und (später) SFTP.</b></p>

---

> **Status:** v0.3 – Strecken erfassen; je Strecke Status aller Stufen und Protokoll der Läufe; lokale Stufen vergleichen und **„Direkt“ übertragen** (Plan, Sammelbestätigung der Löschungen, Journal). FTPS und „Umschalten“ folgen.

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

Voraussetzungen: Rust über rustup (stable, mit `llvm-tools`), `cargo-llvm-cov`, Node.js ≥ 24.15, Xcode Command Line Tools – Details in der [v0.2 Entwicklungsdoku](docs/v0.2%20Entwicklungsdoku.md), Kap. 2.

```bash
npm install
npm run tauri dev                 # App im Entwicklungsmodus
npm test                          # alle Tests (Frontend + Rust)
npm run pruefen                   # Build + Tests mit Abdeckungsgrenze 100 % – vor jedem Commit
```

Fertige App (nur Mac mit Apple Silicon): `npm run app` baut Stellwerk, signiert mit dem Zertifikat „Developer ID Application“ aus dem Schlüsselbund und ersetzt `/Applications/Stellwerk.app`. Stellwerk muss dazu beendet sein.

DMG für eine GitHub-Release: `npm run dmg` baut, signiert, lässt das DMG von Apple beglaubigen und legt es unter `target/release/bundle/dmg/` ab. Voraussetzung: sauberer Git-Stand und einmalig hinterlegte Zugangsdaten (`xcrun notarytool store-credentials stellwerk-notar`).

Konfiguration und Laufzeitdaten: `~/Library/Application Support/de.sieber-bw.stellwerk/` (`config.toml`, `strecken/<id>.toml`, `stellwerk.sqlite`).

## Dokumentation

- [Konzept & Entscheidungen](docs/KONZEPT.md)
- [Architektur](docs/ARCHITEKTUR.md)
- [v0.1 Entwicklungsdoku](docs/v0.1%20Entwicklungsdoku.md) – Grundlagen
- [v0.2 Entwicklungsdoku](docs/v0.2%20Entwicklungsdoku.md) – Vergleichsansicht
- [v0.3 Entwicklungsdoku](docs/v0.3%20Entwicklungsdoku.md) – Übertragen „Direkt“, Journal, Protokoll, Streckenansicht
- Interne Notizen: `docs/intern/`

## Lizenz

MIT – siehe [LICENSE](LICENSE).
