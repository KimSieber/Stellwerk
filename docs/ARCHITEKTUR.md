# Stellwerk – Architekturkonzept

Stand: 06.11.2026 · Status: Entwurf · Bezug: [KONZEPT.md](KONZEPT.md) (Entscheidungen E1–E11) · Abweichungen der Umsetzung stehen in den Entwicklungsdokus je Version

---

## 1. Leitlinien

| # | Leitlinie | Folge |
|---|---|---|
| A1 | **Logik in Rust, Anzeige in React.** Das Frontend rechnet nichts, es zeigt an und löst Befehle aus. | Testbarkeit, Sicherheit, Performance bei großen Dateimengen. |
| A2 | **Kern als eigenständige Bibliothek** (`stellwerk-core`), Tauri ist nur eine dünne Hülle. | Kern ohne UI testbar; später CLI möglich. |
| A3 | **Pipeline statt Seiteneffekte:** Scannen → Vergleichen → Planen → Ausführen. Nur der letzte Schritt verändert etwas. | Dry-Run ist kein Sonderfall, sondern der Normalfall (E4). |
| A4 | **Einbahn im Typsystem:** Quellen sind nur lesbar (`SourceEndpoint`), nur Ziele schreibbar (`TargetEndpoint`). | E3 technisch erzwungen, nicht nur per Konvention. |
| A5 | **Konfiguration lesbar (TOML), Laufzeitdaten in SQLite.** | Strecken sicherbar/versionierbar; Journal und Protokoll abfragbar. |
| A6 | **Jede Ausführung ist fortsetzbar.** Zustand liegt im Journal, nicht im Speicher. | Abbruch, Absturz, Verbindungsverlust beherrschbar. |

---

## 2. Gesamtbild

```
┌──────────────────────────── Tauri-App ─────────────────────────────┐
│  React / TypeScript (UI)                                           │
│   Strecken · Vergleichsansicht · Regeln · Ausführung · Protokoll   │
│          ▲ Events/Channels (Fortschritt)     │ invoke (Befehle)     │
│──────────┼───────────────────────────────────▼──────────────────────│
│  src-tauri  (dünne Hülle: Commands, State, Events, Fenster)        │
│──────────────────────────────────────────────────────────────────── │
│  stellwerk-core (Rust-Bibliothek)                                   │
│   config ─ rules ─ scan ─ diff ─ plan ─ exec ─ journal ─ audit      │
│                      │                     │                        │
│              endpoint: LocalFs · Ftps · (Sftp)                      │
│              secrets: Schlüsselbund      store: SQLite              │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 3. Projektstruktur (Cargo-Workspace)

```
Stellwerk/
├── Cargo.toml                 # [workspace] members = ["crates/*", "src-tauri"]
├── crates/
│   └── stellwerk-core/
│       └── src/
│           ├── lib.rs
│           ├── model/         # Strecke, Stufe, Endpunkt, Regel, Eintrag …
│           ├── config/        # Laden/Speichern TOML, Validierung, Migration
│           ├── rules/         # Regel-Engine (Ebenen, gitignore-Muster)
│           ├── endpoint/      # Traits + LocalFs, Ftps, (Sftp)
│           ├── scan/          # Dateibaum erfassen, NFC, Hashes
│           ├── diff/          # Vergleich → DiffEntry
│           ├── plan/          # Auswahl → Operationen
│           ├── exec/          # Ausführung: Direkt, Umschalten
│           ├── store/         # SQLite: Manifest, Journal, Protokoll
│           ├── secrets/       # Schlüsselbund
│           └── error.rs
├── src-tauri/                 # Tauri-Hülle (commands.rs, events.rs, state.rs)
├── src/                       # React-Frontend
└── docs/
```

---

## 4. Datenmodell

### 4.1 Konfiguration (TOML)

Ablage: `~/Library/Application Support/de.sieber-bw.stellwerk/`

```
config.toml                # globale Einstellungen + globale Standardregeln
strecken/
  mailwerk-web.toml        # eine Datei pro Strecke
  picturesearchview.toml
stellwerk.sqlite           # Laufzeitdaten (Kap. 4.2)
```

Beispiel einer Strecke:

```toml
id      = "mailwerk-web"
name    = "Mailwerk Webseite"
version = 1

[regeln]                       # Ebene „Strecke“
standard_aus = [".git/"]       # globale Vorgabe abgeschaltet
eigene = [
  { muster = "*.psd", typ = "ignorieren" },
]

[[stufen]]
id     = "spiegel"
name   = "Entwicklung → Spiegel"
modus  = "direkt"                         # direkt | umschalten
quelle = { typ = "lokal", pfad = "/Users/kim/Dropbox/Dev/GitHub/Mailwerk/Webseite" }
ziel   = { typ = "lokal", pfad = "/Users/kim/Server/mailwerk" }
regeln.eigene = [
  { muster = "config.php", typ = "schuetzen" },
]

[[stufen]]
id     = "server"
name   = "Spiegel → Server"
modus  = "umschalten"
quelle = { typ = "stufe", stufe = "spiegel" }   # Ziel der Vorstufe ist Quelle
ziel   = { typ = "ftps", host = "ftp.example.de", port = 21, pfad = "/httpdocs",
           benutzer = "kim", tls = "explizit", konto_ref = "stellwerk:mailwerk-web:server" }
rechte = { modus = "fest", ordner = "755", dateien = "644" }
nur_online = "herunterladen"                    # herunterladen | ueberspringen
regeln.eigene = [
  { muster = "logs/",      typ = "schuetzen" },
  { muster = ".htaccess",  typ = "nur_anlegen" },
  { muster = "config.php", typ = "ersetzen", quelle = "config.server.php" },
]
```

Kein Passwort in der Datei – nur `konto_ref` als Schlüssel in den Schlüsselbund (E7).

### 4.2 Laufzeitdaten (SQLite)

```sql
-- Stand des zuletzt übertragenen Inhalts je Stufe (Vergleichsbasis für FTP)
CREATE TABLE manifest (
  strecke_id TEXT, stufe_id TEXT, pfad TEXT,          -- NFC-normalisiert, relativ
  groesse INTEGER, hash BLOB, mtime INTEGER, ist_ordner INTEGER,
  lauf_id TEXT, PRIMARY KEY (strecke_id, stufe_id, pfad)
);

-- Ein Ausführungslauf
CREATE TABLE lauf (
  id TEXT PRIMARY KEY, strecke_id TEXT, stufe_id TEXT,
  modus TEXT, phase TEXT,               -- geplant|hochladen|umschalten|abschluss|fertig|abgebrochen|rollback
  gestartet INTEGER, beendet INTEGER, zusammenfassung TEXT
);

-- Journal: jede geplante Operation eines Laufs mit Status
CREATE TABLE journal (
  lauf_id TEXT, nr INTEGER, op TEXT,    -- mkdir|kopieren|ueberschreiben|loeschen|chmod
  pfad TEXT, schatten TEXT, backup TEXT,
  status TEXT,                          -- offen|uebertragen|umgeschaltet|bereinigt|fehler|zurueckgerollt
  fehler TEXT, aktualisiert INTEGER,
  PRIMARY KEY (lauf_id, nr)
);

-- Gelernte Werte je Stufe (z. B. Parallelität, TLS-Fingerprint, Server-Fähigkeiten)
CREATE TABLE stufen_status (
  strecke_id TEXT, stufe_id TEXT, schluessel TEXT, wert TEXT, aktualisiert INTEGER,
  PRIMARY KEY (strecke_id, stufe_id, schluessel)
);

-- Protokoll: jede Aktion auf Laufwerken/Servern (E8); Einstellungen in Stellwerk nicht
CREATE TABLE protokoll (
  id INTEGER PRIMARY KEY, zeit INTEGER, ebene TEXT,    -- info|warnung|fehler
  strecke_id TEXT, stufe_id TEXT, lauf_id TEXT,
  aktion TEXT, pfad TEXT, details TEXT                  -- details als JSON
);
```

### 4.3 Kern-Typen (Rust, verkürzt)

```rust
pub struct Entry {            // Ergebnis eines Scans
    pub path: RelPath,        // NFC, '/'-getrennt
    pub kind: EntryKind,      // Datei | Ordner | Symlink
    pub size: u64,
    pub mtime: Option<i64>,
    pub hash: Option<Hash>,   // lazy berechnet
    pub cloud_only: bool,     // Dropbox „nur online“
}

pub enum DiffStatus { Neu, Geaendert, Loeschen, Gleich, Geschuetzt, Ignoriert, Konflikt }

pub struct DiffEntry {
    pub path: RelPath,
    pub status: DiffStatus,
    pub quelle: Option<Entry>,
    pub ziel: Option<Entry>,
    pub regel: Option<RuleRef>,   // welche Regel greift (Herkunft für die UI)
    pub warnungen: Vec<Warnung>,  // Groß/Klein, Symlink, Konfliktkopie, nur online …
    pub ausgewaehlt: bool,        // Vorgabe: true bei Neu/Geaendert/Loeschen
}

pub enum Op { Mkdir(RelPath), Kopieren(RelPath), Ueberschreiben(RelPath),
              Loeschen(RelPath), Chmod(RelPath, Mode) }
```

---

## 5. Endpunkte

```rust
#[async_trait]
pub trait SourceEndpoint: Send + Sync {
    async fn scan(&self, rules: &RuleSet, cb: &dyn ScanProgress) -> Result<Vec<Entry>>;
    async fn open_read(&self, p: &RelPath) -> Result<Box<dyn AsyncRead + Unpin + Send>>;
    async fn hash(&self, p: &RelPath) -> Result<Hash>;
}

#[async_trait]
pub trait TargetEndpoint: SourceEndpoint {
    async fn write(&self, p: &RelPath, data: &mut (dyn AsyncRead + Unpin + Send), size: u64) -> Result<()>;
    async fn mkdir(&self, p: &RelPath) -> Result<()>;
    async fn rename(&self, from: &RelPath, to: &RelPath) -> Result<()>;
    async fn remove(&self, p: &RelPath) -> Result<()>;
    async fn set_mode(&self, p: &RelPath, mode: Mode) -> Result<ModeResult>; // „nicht unterstützt“ ist kein Fehler
    fn capabilities(&self) -> Capabilities; // chmod, mtime setzen, resume, rename-überschreibt …
}
```

- Die Quelle einer Stufe wird **immer als `SourceEndpoint`** übergeben – auch wenn sie technisch schreibbar wäre (A4).
- **LocalFs:** `walkdir`/`ignore`, mtime übernehmen (`filetime`), Erkennung „nur online“ (macOS: Dateiattribute/`SF_DATALESS`).
- **Ftps:** `suppaftp` (async, rustls), immer `TYPE I`, explizites TLS Pflicht, `MLSD` bevorzugt (Fallback `LIST`), `REST` für Wiederaufnahme. **Adaptive Parallelität** über einen Verbindungspool (Kap. 8.1, Entscheidung 2 in Kap. 16).
- **Sftp:** später über `russh`; gleiche Schnittstelle.

---

## 6. Regel-Engine

- Drei Ebenen: **Global → Strecke → Stufe**. Jede Ebene kann Regeln der darüberliegenden abschalten und eigene ergänzen.
- Muster im **gitignore-Format** (`ignore::gitignore`), innerhalb einer Ebene gilt die letzte passende Regel; tiefere Ebene schlägt höhere.
- Ergebnis je Pfad: `(RegelTyp, RuleRef { ebene, index, muster })` → Herkunft in der UI sichtbar.
- **Ignorieren** wirkt schon beim Scan (Performance), die übrigen Typen beim Vergleich bzw. Planen.

---

## 7. Pipeline

```
 scan(quelle) ─┐
               ├─► diff ─► [UI: Auswahl, Regeln setzen] ─► plan ─► [UI: Bestätigung] ─► exec
 scan(ziel) ───┘                                           │
   oder manifest                                           └─ Löschungen? → Sammelbestätigung (E5)
```

1. **Scan** – Quelle und Ziel (lokal umgesetzt: nacheinander, s. v0.2-Doku Kap. 8). Ziel bei FTP standardmäßig aus dem **Manifest**; „Vollabgleich“ liest den Server. Pfade nach NFC normalisiert.
2. **Diff** – Abgleich über Pfad; „geändert“ über Größe, dann Hash (lokal) bzw. Hash gegen Manifest (FTP). Regeln werden angewendet, Warnungen ergänzt (Groß/Klein-Kollision, Symlink, Konfliktkopie, nur online inkl. Volumen).
3. **Plan** – aus ausgewählten Einträgen werden geordnete Operationen: Ordner anlegen (auch leere) → Dateien → Rechte → Löschungen (tiefste Pfade zuerst). Rechte nach Stufen-Einstellung (nicht setzen / fest / Muster); `777` erzeugt eine Warnung, kein Verbot. „Ersetzen durch“ tauscht die Quelldatei.
4. **Exec** – Strategie je Modus (Kap. 8). Jeder Schritt wird vorher im Journal angelegt und nachher aktualisiert. Nach Erfolg: Manifest aktualisieren.

Scan und Diff laufen bei jedem Öffnen einer Stufe automatisch – die Vorschau ist damit immer aktuell.

---

## 8. Ausführung

```rust
trait Strategy {
    async fn run(&self, plan: &Plan, target: &dyn TargetEndpoint, j: &Journal, cb: &dyn ExecProgress) -> Result<RunResult>;
}
```

**Direkt** – Operationen in Plan-Reihenfolge, parallel über den Worker-Pool (Kap. 8.1); Fehler je Datei werden gesammelt, der Lauf läuft weiter (abschaltbar: „bei Fehler stoppen“).

### 8.1 Adaptive Parallelität (von Anfang an eingeplant)

- Ausführung grundsätzlich über einen **Worker-Pool** mit *n* Verbindungen – auch lokal (dort Threads statt Verbindungen). *n* = 1 ist nur ein Spezialfall, kein eigener Codepfad.
- **Start hoch, dann herunterregeln:** Startwert pro Stufe = zuletzt erfolgreicher Wert, beim ersten Mal ein Maximalwert (Standard 8, einstellbar).
- **Downgrade** bei Überlastungssignalen: FTP `421` (too many connections), `530` bei sonst gültigem Login, Verbindungsabbrüche, Timeouts, TLS-Handshake-Fehler unter Last → *n* halbieren (min. 1), betroffene Operation zurück in die Warteschlange, kein Fehler für den Nutzer.
- **Behutsames Hochregeln** nach einer Serie fehlerfreier Übertragungen (+1), nie über den Maximalwert.
- Gelernter Wert wird **pro Stufe gespeichert** (SQLite, Tabelle `stufen_status`) und im Protokoll vermerkt; in der UI sichtbar („4 Verbindungen“) und manuell überschreibbar (fester Wert).
- **Reihenfolge-Abhängigkeiten** bleiben gewahrt: Ordner anlegen vor Dateien darin, Löschungen von Dateien vor ihren Ordnern; der Plan liefert dazu Abhängigkeiten, der Pool arbeitet nur „freie“ Operationen ab.
- Jeder Worker schreibt seinen Fortschritt ins Journal → Fortsetzen funktioniert unabhängig von *n*.

### 8.2 Modi

**Umschalten** – drei Phasen, jede einzeln vom Nutzer ausgelöst:

| Phase | Operationen | Abbruch |
|---|---|---|
| Hochladen | `write(pfad.dpl-<lauf>)` | Schattendateien löschen, Live-Stand unberührt |
| Umschalten | `rename(pfad → pfad.bak-<lauf>)`, `rename(pfad.dpl → pfad)`, Löschungen als `rename → .bak` | Rollback aus Journal |
| Abschluss | `remove(*.bak-<lauf>)` oder Rollback | – |

Optional: **Wartungsseite** während der Umschaltphase (Stufen-Einstellung: Datei, die vor Phase 2 aktiviert und danach entfernt wird, z. B. `.maintenance` / `.htaccess`-Weiche).

**Verwaiste Dateien:** Der Diff erkennt `*.dpl-*` / `*.bak-*` ohne offenen Lauf und zeigt sie als Warnung mit Aufräum-Angebot.

Hinweis: Manche FTP-Server überschreiben beim Umbenennen nicht – deshalb immer erst Original → `.bak`, dann Schatten → Original.

**Fortsetzen:** Beim Start prüft die App auf Läufe mit `phase ≠ fertig/abgebrochen` und bietet Fortsetzen / Rollback / Aufräumen an. Abgleich Journal ↔ tatsächlicher Zustand (Server-Listing) vor jeder Fortsetzung.

**Abbrechen:** kooperativ über `CancellationToken`; laufende Einzelübertragung wird sauber beendet.

---

## 9. Schnittstelle Rust ↔ React

### Commands (`invoke`)

| Command | Zweck |
|---|---|
| `strecken_liste`, `strecke_laden`, `strecke_speichern`, `strecke_loeschen` | Verwaltung |
| `regel_setzen`, `regel_entfernen`, `regel_umschalten` | aus Kontextmenü / Regelübersicht |
| `konto_speichern`, `verbindung_testen` | Schlüsselbund, Test inkl. Fähigkeiten (chmod, MLSD …) |
| `vergleich_starten(stufe, vollabgleich)` | Scan + Diff, liefert `vergleich_id` |
| `auswahl_setzen(vergleich_id, pfade, an)` | Häkchen |
| `plan_erstellen(vergleich_id)` | liefert Plan-Zusammenfassung inkl. Löschliste |
| `lauf_starten(plan_id, bestaetigt_loeschen)` | Ausführung |
| `phase_ausloesen(lauf_id, phase)` / `lauf_abbrechen` / `lauf_rollback` | Steuerung |
| `offene_laeufe`, `protokoll_abfragen(filter)` | Übersicht |

### Events / Channels

`scan_fortschritt`, `diff_bereit`, `exec_fortschritt` (Datei, Bytes, Gesamt), `exec_fehler`, `lauf_status`.
Fortschritt über **Tauri-Channels** und gedrosselt (≈ 10 Updates/s), damit die UI bei Tausenden Dateien flüssig bleibt.

Große Diff-Listen werden **seitenweise** abgefragt (`diff_seite(vergleich_id, offset, limit, filter)`), der Diff selbst bleibt im Rust-State.

---

## 10. Frontend

- **React + TypeScript + Vite**, State mit **Zustand**, Server-State/Abfragen mit **TanStack Query**.
- **Virtualisierte Listen** (`@tanstack/react-virtual`) für die Vergleichsansicht – zeilengleiche Doppelspalte, Baum ein-/ausklappbar.
- Typen für Commands/Events aus Rust generiert (`specta` + `tauri-specta`) → keine Abweichungen zwischen Backend und Frontend.

Ansichten:
1. **Startseite** – Strecken als Karten mit Stufen; Klick öffnet die Strecke. Bewusst ohne Status (kein Vergleich aller Strecken beim Start)
1a. **Strecke** (seit v0.3) – alle Stufen untereinander, beim Öffnen im Hintergrund verglichen: je Stufe Status (was zu tun ist) und Protokoll der Läufe bis zu jedem Schritt
2. **Stufe** – Vergleichsansicht nach Farbschema aus KONZEPT Kap. 4 (🟢 neu · 🟡 geändert · 🔴 löschen · ⚪ geschützt/ignoriert, zeilengleich), Filter (nur Änderungen / alle / Warnungen), Aktionsleiste
3. **Ausführung** – Phasen, Fortschritt, offene Dateien, Abbrechen/Rollback
4. **Regeln** – Übersicht mit Herkunft und Schaltern
5. **Strecke bearbeiten** – Stufen, Endpunkte, Modus, Rechte, Verbindungstest
6. **Protokoll** – in der Streckenansicht je Stufe (seit v0.3)

---

## 11. Sicherheit

- Passwörter nur im Schlüsselbund (`keyring`, Dienst `de.sieber-bw.stellwerk`, Konto = `konto_ref`); nie in TOML, Logs oder Events.
- TLS-Zertifikate werden geprüft; abweichende Zertifikate (selbstsigniert beim Hoster) nur nach expliziter Bestätigung mit gespeichertem Fingerprint je Stufe.
- Tauri-Capabilities minimal: kein Dateisystemzugriff aus dem Frontend, alles über Commands.
- Pfad-Validierung im Kern: keine `..`-Ausbrüche, alle Operationen relativ zur Stufenwurzel.

---

## 12. Fehlerbehandlung & Protokoll

- Ein Fehlertyp `StellwerkError` (`thiserror`) mit Kategorien: Konfiguration, Verbindung, Zugriff, Konflikt, Abbruch.
- Fehler je Datei stoppen nicht den Lauf (außer bei Verbindungsverlust → automatischer Wiederverbindungsversuch, dann Pause).
- Technisches Log zusätzlich über `tracing` in Datei (rotierend); das **fachliche Protokoll** ist die SQLite-Tabelle.

---

## 13. Teststrategie

| Ebene | Vorgehen |
|---|---|
| Regeln, Diff, Plan | Unit-Tests mit In-Memory-Endpunkt (`MemFs`), TDD |
| LocalFs | Tests auf Temp-Verzeichnissen (`tempfile`), inkl. Umlaute NFD/NFC, Groß/Klein |
| FTPS | Integrationstests gegen lokalen FTPS-Server im Docker-Container (z. B. `pure-ftpd`/`vsftpd` mit TLS) |
| Parallelität | `MemFs` mit simuliertem Verbindungslimit → Downgrade/Upgrade prüfen |
| Umschalten/Rollback | Fehlerinjektion im `MemFs` (Verbindungsabbruch nach n Operationen) → Fortsetzen prüfen |
| UI | Komponenten-Tests (Vitest), wenige E2E-Tests |

---

## 14. Abhängigkeiten (geplant)

Rust: `tokio`, `async-trait`, `serde`, `toml`, `rusqlite` (bundled), `suppaftp` (async-rustls), `keyring`, `ignore`, `walkdir`, `blake3`, `unicode-normalization`, `filetime`, `thiserror`, `tracing`, `tokio-util` (CancellationToken), `specta`/`tauri-specta`; später `russh`.
Frontend: `react`, `typescript`, `vite`, `zustand`, `@tanstack/react-query`, `@tanstack/react-virtual`, `vitest`.

---

## 15. Umsetzungsreihenfolge

1. Workspace + `stellwerk-core` Gerüst, Modell, TOML-Konfiguration
2. Regel-Engine + Scan LocalFs + Diff (mit Tests)
3. Tauri-Hülle, Startseite, Vergleichsansicht (lokal → lokal)
4. Plan + Ausführung „Direkt“ lokal über den Worker-Pool, Journal, Protokoll
5. Schlüsselbund + FTPS-Endpunkt + Manifest + adaptive Parallelität (Downgrade/Upgrade)
6. Strategie „Umschalten“ inkl. Fortsetzen/Rollback
7. Feinschliff: Rechte, Warnungen, Dropbox „nur online“
8. SFTP

---

## 16. Architekturentscheidungen (geklärt 06.11.2026)

1. **App-Identifier:** `de.sieber-bw.stellwerk` – bestätigt.
2. **Verbindungen:** **Adaptive Parallelität** – möglichst viele Verbindungen, bei Überlastung automatisch herunterregeln (Kap. 8.1). Grund: FTP baut pro Datei eine eigene TLS-Datenverbindung auf, bei vielen kleinen Dateien dominiert die Latenz. Von Anfang an in der Architektur (Worker-Pool), Umsetzung ab Schritt 4.
3. **Kein Manifest für lokale Ziele.** Lokal wird immer direkt verglichen (Größe, mtime, bei Bedarf Hash). Manifest nur für entfernte Ziele (FTPS/SFTP).
4. **Stufen werden immer einzeln ausgeführt.** Keine „Durchfahrt“ über mehrere Stufen – die Zwischenstufe dient dem Qualitäts-Check. Die UI bietet nach Abschluss einer Stufe lediglich an, die nächste Stufe zu *öffnen* (Vergleich anzeigen), nie sie auszuführen.
