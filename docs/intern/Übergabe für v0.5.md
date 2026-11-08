# Übergabe für v0.5 – Stellwerk (Stand 08.11.2026)

Einstieg für einen neuen Chat. Zuerst dieses Dokument, dann [`docs/v0.4 Entwicklungsdoku.md`](../v0.4%20Entwicklungsdoku.md) (was v0.4 gebracht hat, Erkenntnisse vom ersten echten Lauf), bei Bedarf die älteren Entwicklungsdokus, KONZEPT und ARCHITEKTUR.

## 1. Arbeitsregeln (verbindlich)

- **Claude committet nie.** Nach jedem Schritt einen Commit-Text liefern; Kim committet nur Stände mit **sichtbarer** Veränderung in der App.
- **Arbeit in Schritte schneiden, die am Ende etwas Prüfbares haben.** Vorher kurz gliedern (z. B. a/b/c), dann umsetzen. Nach der Umsetzung **stoppen** und das Ergebnis besprechen. Kim legt die Schrittgröße fest (am 08.11. erst einzeln, später „3a und 3b am Stück“).
- **Kim führt `npm run pruefen` selbst aus** (Claude nicht). Claude prüft vorher gezielt (betroffene Tests, Abdeckung der geänderten Teile), damit es keine Rückläufer gibt.
- Testpflicht: jede Funktion und jede Funktionsstrecke mit Testfall, 100 % Abdeckung (Rust Funktionen/Zeilen, Frontend auch Zweige).
- **Terminal-Befehle für Kim einzeln**, ein Befehl pro Schritt, Ausgabe abwarten. Platzhalter vermeiden – lieber die echten Werte vorher erfragen/ermitteln (Kim hat `STRECKE` wörtlich übernommen).
- **Keine Befehle vorschlagen, die Passwörter anzeigen** (z. B. `curl -v` zeigt `PASS …`).
- Antworten auf den Punkt bringen; bei Fehlersuche: Ursache + Behebung, langsam und schrittweise.
- In der Oberfläche **nicht „Manifest“** sagen – „letzter bekannter Stand“.

## 2. Ordner und Übertragen

- Ordnerzugriff gilt nur je Chat: Kim gibt `/Users/kim/Dropbox/Dev/GitHub/Stellwerk` über „Add folder“ frei.
- Arbeitskopie: `GIT_LFS_SKIP_SMUDGE=1 git clone --depth 1 https://github.com/KimSieber/stellwerk` – danach Gesamt-Prüfsumme mit Kims Ordner vergleichen (Kim committet, pusht aber evtl. nicht sofort).
- **Vor dem Schreiben** Gesamt-Prüfsumme von Kims Ordner prüfen; geänderte Dateien **einzeln** schreiben (keine Archive, kein `_to_delete`; > 50 Dateien vorher melden).
- **Jede Übertragung über einen neuen Ablageort** (`/mnt/user-data/outputs/<neuer-name>/…`) und **danach im Ordner per Prüfsumme nachsehen**: Am 08.11. meldete das Werkzeug „geschrieben“, ließ aber die alten Dateien stehen, als derselbe Ablageort wiederverwendet wurde.
- Gesamt-Prüfsumme (auf Kims Seite in `$HOME/mnt/Stellwerk`):
  `find . \( -name node_modules -o -name target -o -name dist -o -name .git -o -name assets -o -name gen -o -path ./docs/intern -o -path ./src-tauri/icons \) -prune -o -type f ! -name .DS_Store -print0 | LC_ALL=C sort -z | xargs -0 sha256sum | sed 's#  \./#  #' | sha256sum`
- Linux-Umgebung: `apt-get update` und `apt-get install -y libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libssl-dev`, `rustup component add llvm-tools-preview`, `cargo install cargo-llvm-cov --locked`, `npm ci --ignore-scripts`.

## 3. Stand

v0.4.0 ist abgeschlossen: Stellwerk vergleicht und überträgt per FTPS (erster echter Lauf bei manitu am 08.11.2026). Einzelheiten: v0.4-Entwicklungsdoku.

Kims Testziel bei manitu: `ngcobalt418.manitu.net`, Port 21 (explizites FTPS; auf 23 läuft SFTP), Unterbenutzer `ftp200043125` mit eigenem Ordner `/web/sieber-bw.de/test/` → **Serverpfad der Stufe `/`**. Strecke `test`, Stufe `stufe-1`. Daneben die Strecke `mailwerk`.

Bei FTPS-Problemen zuerst das Diagnose-Programm: `cargo run -p stellwerk --example ftps_diagnose -- test stufe-1` (zeigt jeden FTP-Befehl mit Zeitstempel).

## 4. Offen / Ideen

| Thema | Herkunft |
|---|---|
| Neu vergleichen, wenn Stellwerk in den Vordergrund kommt (Streckenansicht offen, letzter Vergleich > 1 Min.; FTPS gegen den letzten bekannten Stand) | Entscheidung v0.4, nicht umgesetzt |
| Wiederholungen der Datenverbindung im Journal vermerken | v0.4, angekündigt, bewusst ausgelassen |
| Adaptive Parallelität (statt fest 2 Verbindungen) | Architektur, verschoben |
| „Inhalte prüfen“: Dateien mit unbekanntem Inhalt einmal herunterladen und hashen | Entscheidung v0.4 („Hash nur auf Wunsch“) |
| Klarere Meldung bei `550 Permission denied` („Keine Schreibrechte in … – Serverpfad bzw. Rechte prüfen“) | 08.11. |
| **SFTP** als weiteres Ziel (manitu Port 23; eine Verbindung, keine passiven Ports) | Kim 08.11., eigene Version |
| **Samba-Freigaben (SMB)** als Quelle/Ziel in Strecken | Kims Wunsch 08.11. |
| Webseite stellwerk.sieber-bw.de, „Über Stellwerk“ wie bei Mailwerk | früher vorgemerkt |

**Umfang von v0.5 zu Beginn mit Kim festlegen** (KONZEPT-Fahrplan prüfen und mit dieser Liste abgleichen). Zu SMB vorab klären: Reicht eine in macOS eingehängte Freigabe (`/Volumes/…`, dann ist sie ein lokaler Ordner und funktioniert schon heute), oder soll Stellwerk die Freigabe selbst verbinden (Anmeldung, Schlüsselbund, Wiederverbinden)?

## 5. Startprompt für den neuen Chat

> Wir arbeiten an meinem Projekt **Stellwerk** (`/Users/kim/Dropbox/Dev/GitHub/Stellwerk`, Ordner ist freigegeben). Lies bitte zuerst `docs/intern/Übergabe für v0.5.md`, dann `docs/v0.4 Entwicklungsdoku.md`. Gleiche deine Arbeitskopie mit meinem Ordner ab. Dann legen wir gemeinsam den Umfang von v0.5 fest – noch nichts umsetzen. Du committest nie; du lieferst einen Commit-Text. Terminal-Befehle für mich einzeln.
