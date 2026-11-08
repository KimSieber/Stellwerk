# Übergabe für v0.4 (Fortsetzung) – Stellwerk (Stand 08.11.2026, 01:50)

Einstieg für einen neuen Chat. Zuerst dieses Dokument, dann [`docs/v0.4 Entwicklungsdoku.md`](../v0.4%20Entwicklungsdoku.md) (alles, was in v0.4 bisher entstand), dann bei Bedarf `Übergabe für v0.4.md` (Arbeitsregeln, Orte), KONZEPT und ARCHITEKTUR.

## 1. Arbeitsregeln (unverändert, verbindlich – siehe `Übergabe für v0.4.md` Kap. 3)

- **Claude committet nie.** Nach jedem Durchgang Commit-Text liefern; Kim committet nur Stände mit sichtbarer Veränderung.
- **Kleine Schritte:** höchstens **ein** Schritt (s. Kap. 4), dann übertragen und Kims Test abwarten. (Am 07./08.11. wurden die Schritte zu groß – Kim hat das zu Recht angemahnt.)
- Testpflicht, 100 % Abdeckung, Gegenproben. Terminal-Befehle einzeln.
- **Ordnerzugriff:** Er gilt nur je Chat. Kim gibt `/Users/kim/Dropbox/Dev/GitHub/Stellwerk` über „Add folder“ in der Claude-App frei (eine automatische Anfrage wurde am 07.11. abgelehnt). Klonen von GitHub (öffentlich) funktioniert als Arbeitskopie.
- **Übertragen:** Vor dem Schreiben Gesamt-Prüfsumme von Kims Ordner gegen den letzten bekannten Stand vergleichen (Befehl in Kap. 5); geänderte Dateien einzeln schreiben; danach Gesamt-Prüfsumme vergleichen.

## 2. Stand

| Schritt | Stand |
|---|---|
| Paket 1+2: Endpunkte, FTPS-Verbindung, „Verbindung testen“ | ✅ committet von Kim |
| Paket 3: FTPS schreiben, Schema 2 (`manifest`, `stufen_status`) | ✅ übertragen, Kim: `npm run pruefen` grün auf dem Mac |
| **Schritt 1 = Paket 4a:** Server einlesen, Vergleich gegen Manifest, Schema 3, Hash/Zeit aus dem Lauf | ✅ übertragen 08.11. 01:50 – **Kim lässt `npm run pruefen` laufen** (noch offen) |
| **Schritt 2:** FTPS vergleichen in der Oberfläche | ⏳ als Nächstes |
| **Schritt 3:** „Übertragen …“ für FTPS | ⏳ |
| danach: adaptive Parallelität, Neu vergleichen bei Rückkehr ins Fenster (ab 1 Min.) | ⏳ |

In der App ist seit „Verbindung testen“ noch nichts Neues sichtbar. Version bleibt 0.3.0 bis zum ersten FTPS-Lauf.

Kims Server (manitu): `ngcobalt418.manitu.net`, **Port 21** (auf 23 läuft SFTP), Serverpfad `/web/mailwerk.sieber-bw.de/`, ProFTPD, TLS 1.3, Zertifikat vom System anerkannt, MLST/MFMT/SITE CHMOD/REST.

## 3. Zur Prüfung vorgelegt (Kim hat noch nicht entschieden)

1. Bestätigter Zertifikat-Fingerprint in der Strecken-Konfiguration (`ziel.zertifikat`) statt in `stufen_status`.
2. Manifest entsteht schon beim **ersten Einlesen des Servers** (ohne Hashes), nicht erst mit dem ersten Lauf – sonst erschienen unveränderte Dateien danach als „neu“.
3. FTPS vorerst mit **2 Verbindungen** fest (bis zur adaptiven Parallelität).

## 4. Geplante Schritte 2 und 3 (Entwurf war begonnen, wurde **zurückgenommen** – neu umsetzen)

### Schritt 2 – FTPS vergleichen (sichtbar, ändert nichts am Server)

Kern:
- `StellwerkError::KeinManifest` (Kategorie `kein_manifest`).
- `ansicht::ziel_kennung(&Endpunkt) -> PathBuf` (Ordner bzw. `ftps://host:port/pfad`), in `stufe_vergleichen_entfernt` statt des eigenen `format!` verwenden; Test für beide Arme.

Befehle (`commands.rs`):
- `vergleich_starten(strecke, stufe, fortschritt, server: Option<bool>)`. FTPS-Stufe: ohne `server` gegen das Manifest; ist in `stufen_status` kein `manifest_seit` → `KeinManifest`. Mit `server`: Passwort aus dem Schlüsselbund, `Ftps::new`, `EntfernterStand::Server`, danach `manifest_ersetzen` + `status_setzen("manifest_seit", jetzt)`. DB-Sperre nicht während des Netzzugriffs halten.
- `VergleichStart.ziel_stand: Option<&str>` (`manifest` | `server`), nur bei FTPS.
- Hilfe `passwort(state, konto)` (auch in `verbindung_testen` nutzen).
- `plan_erstellen`: „gleiche Ordner“ über `ziel_kennung` prüfen (sonst schlägt der Plan für FTPS fehl).

Oberfläche:
- `api.ts`: `vergleichStarten(…, server = false)`, `ziel_stand`, Kategorie `kein_manifest`, Warnung `inhalt_nicht_geprueft`.
- `format.ts`: Text für `inhalt_nicht_geprueft` („Inhalt nicht geprüft – nur Größe verglichen“).
- `StreckeAnsicht.tsx`: FTPS-Stufen wie lokale vergleichen (Status „ftps“ und Text „Vergleich folgt mit dem Manifest (v0.4)“ entfernen). Bei `kein_manifest`: „Noch nie mit dem Server verglichen.“ + Knopf **„Mit Server vergleichen …“** (`server = true`).
- `VergleichAnsicht.tsx`: bei `ziel_stand` Knopf **„Server neu lesen“** (Vollabgleich, `server = true`); Hinweis, woher der Stand kommt.
- „Übertragen …“ für FTPS bleibt in Schritt 2 gesperrt (Hinweis „folgt“).

### Schritt 3 – FTPS übertragen (sichtbar: erster echter Upload)

- `endpoint::ftps::FTPS_WORKER = 2`; `worker_fuer(&Endpunkt)` im Befehl (auch in der Plan-Vorschau).
- `lauf_starten`: Ziel der Stufe neu laden und gegen die Kennung des Vergleichs prüfen (`ziel_der_stufe`); `uebertragen` erhält `&Endpunkt`.
- `uebertragen` bei FTPS: Passwort und gelerntes `umbenennen_ueberschreibt` (`ja`/`nein`) **vor** `lauf_anlegen` holen; `Ftps::mit_umbenennen_ueberschreibt`; Ziel `&dyn ZielEndpunkt` (Ftps oder Lokal).
- Journal-Rückmeldung (`op, status, meldung, uebertragen`): bei FTPS und `Erledigt` Manifest fortschreiben – Datei: `manifest_setzen` (Bytes, Hash, `quelle_mtime_ns`, `lauf_id`); Ordner anlegen: Eintrag `ist_ordner`; Löschen: `manifest_entfernen`; Rechte: nichts. Fehler dabei → Journalfehler (Lauf endet).
- Nach dem Lauf: `status_setzen("umbenennen_ueberschreibt", ja|nein)`, wenn gelernt.
- `verbindung_testen` liefert zusätzlich `umbenennen_ueberschreibt` (Stufe aus `konto_ref` = `stellwerk:<strecke>:<stufe>`); Anzeige im Verbindungstest „Ersetzen per Umbenennen: ja/nein/noch nicht bekannt“.
- Befehlstests: gegen `stellwerk_testserver` (ist schon dev-dependency von `src-tauri`), mit `zertifikat` = Fingerprint des Testservers.

## 5. Technik

- Arbeitskopie: `GIT_LFS_SKIP_SMUDGE=1 git clone --depth 1 https://github.com/KimSieber/stellwerk` – **Achtung:** GitHub enthält nur Kims letzten Commit (Paket 1+2). Paket 3 und 4a liegen nur in Kims Ordner → Dateien aus Kims Ordner bereitstellen (`device_stage_files`, ≤ 50 je Aufruf) bzw. die Arbeitskopie danach mit ihnen überschreiben.
- Linux-Umgebung: `apt-get install -y libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libssl-dev`, `rustup component add llvm-tools-preview`, `cargo install cargo-llvm-cov --locked`, `npm ci --ignore-scripts`.
- Gesamt-Prüfsumme (identisch auf beiden Seiten ausführen; auf Kims Seite in `$HOME/mnt/Stellwerk`):
  `find . \( -name node_modules -o -name target -o -name dist -o -name .git -o -name assets -o -name gen -o -path ./docs/intern -o -path ./src-tauri/icons \) -prune -o -type f ! -name .DS_Store -print0 | LC_ALL=C sort -z | xargs -0 sha256sum | sed 's#  \./#  #' | sha256sum`
  Stand nach Schritt 1: `0be5477b61bf03efbe332746039ac61e8ba94fba5b4738d417dee6d9cfdd746b`.
- Erkenntnisse zu Abdeckung und Testserver: v0.4-Entwicklungsdoku Kap. 7.

## 6. Startprompt für den neuen Chat

> Wir arbeiten an meinem Projekt **Stellwerk** (`/Users/kim/Dropbox/Dev/GitHub/Stellwerk`, Ordner ist freigegeben). Lies bitte zuerst `docs/intern/Übergabe für v0.4 Fortsetzung.md`, dann `docs/v0.4 Entwicklungsdoku.md`. Hol dir den Stand aus meinem Ordner (GitHub ist nicht aktuell). Dann Schritt 2 (FTPS vergleichen in der Oberfläche) – nur diesen einen Schritt, danach übertragen und auf meinen Test warten. Du committest nie; du lieferst einen Commit-Text. Terminal-Befehle für mich einzeln.
