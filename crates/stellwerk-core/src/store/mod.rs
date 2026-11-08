//! Laufzeitdaten in SQLite (ARCHITEKTUR Kap. 4.2): Läufe, Journal, Protokoll.
//!
//! - **Lauf:** eine Ausführung einer Stufe mit Phase und Zusammenfassung.
//! - **Journal:** jede geplante Operation eines Laufs. Sie wird vor dem Start mit
//!   Status `offen` angelegt und danach aktualisiert (`erledigt`, `fehler`,
//!   `uebersprungen`) – so ist jede Veränderung am Ziel nachvollziehbar.
//! - **Protokoll:** jede Aktion auf Laufwerken und Servern (E8) – Beginn, Fehler und Ende
//!   der Läufe. Einstellungen in Stellwerk selbst werden bewusst nicht protokolliert.
//!
//! - **Manifest** (seit Schema 2, v0.4): Stand des zuletzt übertragenen Inhalts je
//!   entfernter Stufe (E11) – Vergleichsbasis statt Server-Zeitstempeln.
//! - **Stufen-Status** (seit Schema 2): Gelerntes je Stufe, z. B. ob der Server beim
//!   Umbenennen überschreibt oder wie viele Verbindungen er verträgt.
//!
//! Zeiten in Millisekunden seit 1970; sie werden übergeben, damit Tests sie festlegen können.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, ErrorCode, OptionalExtension, Row};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{Result, StellwerkError};
use crate::model::{Eintrag, EintragArt, Hash, Modus, RelPath};
use crate::plan::{OpArt, Plan, PlanZusammenfassung};

/// Dateiname im App-Ordner (neben `config.toml`).
pub const DATEINAME: &str = "stellwerk.sqlite";

/// Stand des Schemas (`PRAGMA user_version`).
const SCHEMA_VERSION: i64 = 5;

const SCHEMA: &str = "
CREATE TABLE lauf (
  id TEXT PRIMARY KEY,
  strecke_id TEXT NOT NULL,
  stufe_id TEXT NOT NULL,
  modus TEXT NOT NULL,
  phase TEXT NOT NULL,
  gestartet INTEGER NOT NULL,
  beendet INTEGER,
  zusammenfassung TEXT NOT NULL,
  ergebnis TEXT
);
CREATE INDEX lauf_stufe ON lauf (strecke_id, stufe_id, gestartet);

CREATE TABLE journal (
  lauf_id TEXT NOT NULL REFERENCES lauf (id),
  nr INTEGER NOT NULL,
  op TEXT NOT NULL,
  pfad TEXT NOT NULL,
  quelle TEXT,
  modus TEXT,
  groesse INTEGER NOT NULL,
  schatten TEXT,
  backup TEXT,
  status TEXT NOT NULL,
  fehler TEXT,
  aktualisiert INTEGER NOT NULL,
  PRIMARY KEY (lauf_id, nr)
);

CREATE TABLE protokoll (
  id INTEGER PRIMARY KEY,
  zeit INTEGER NOT NULL,
  ebene TEXT NOT NULL,
  strecke_id TEXT,
  stufe_id TEXT,
  lauf_id TEXT,
  aktion TEXT NOT NULL,
  pfad TEXT,
  details TEXT
);
CREATE INDEX protokoll_stufe ON protokoll (strecke_id, stufe_id, id);
CREATE INDEX protokoll_lauf ON protokoll (lauf_id, id);
";

/// Ergänzung in Schema 2 (v0.4); vorhandene Datenbanken werden beim Öffnen hochgestuft.
const SCHEMA_2: &str = "
CREATE TABLE manifest (
  strecke_id TEXT NOT NULL,
  stufe_id TEXT NOT NULL,
  pfad TEXT NOT NULL,
  ist_ordner INTEGER NOT NULL,
  groesse INTEGER NOT NULL,
  hash BLOB,
  mtime_ms INTEGER,
  lauf_id TEXT,
  PRIMARY KEY (strecke_id, stufe_id, pfad)
);

CREATE TABLE stufen_status (
  strecke_id TEXT NOT NULL,
  stufe_id TEXT NOT NULL,
  schluessel TEXT NOT NULL,
  wert TEXT NOT NULL,
  aktualisiert INTEGER NOT NULL,
  PRIMARY KEY (strecke_id, stufe_id, schluessel)
);
";

/// Schema 3 (v0.4): Zeit der Quelle in Nanosekunden wie beim Scan – sonst gälte eine
/// unveränderte Datei nie als „gleiche Zeit“ und müsste bei jedem Vergleich gelesen werden.
const SCHEMA_3: &str = "ALTER TABLE manifest RENAME COLUMN mtime_ms TO mtime_ns;";

/// Schema 4 (v0.5): Hinweis je Operation im Journal (z. B. wiederholte Datenverbindung).
const SCHEMA_4: &str = "ALTER TABLE journal ADD COLUMN hinweis TEXT;";

/// Schema 5 (v0.5, Umschalten): Hash und Zeit der Quelle je hochgeladener Datei (für
/// den letzten bekannten Stand beim späteren Umschalten) und die Sicherung des
/// Manifests vor dem Umschalten (für den Rollback).
const SCHEMA_5: &str = "
ALTER TABLE journal ADD COLUMN hash BLOB;
ALTER TABLE journal ADD COLUMN mtime_ns INTEGER;
CREATE TABLE manifest_sicherung (
  lauf_id TEXT NOT NULL,
  strecke_id TEXT NOT NULL,
  stufe_id TEXT NOT NULL,
  pfad TEXT NOT NULL,
  ist_ordner INTEGER NOT NULL,
  groesse INTEGER NOT NULL,
  hash BLOB,
  mtime_ns INTEGER,
  eintrag_lauf_id TEXT,
  PRIMARY KEY (lauf_id, pfad)
);
";

/// Ein Eintrag im Manifest: so liegt der Pfad nach Stellwerks letztem Lauf am Ziel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ManifestEintrag {
    pub pfad: RelPath,
    pub ist_ordner: bool,
    pub groesse: u64,
    /// BLAKE3 des übertragenen Inhalts; fehlt, wenn nur die Größe bekannt ist
    /// (Erstvergleich ohne Herunterladen).
    pub hash: Option<Hash>,
    /// Änderungszeit der Quelle beim Übertragen (ns) – gleiche Zeit und Größe heißt
    /// „unverändert“, ohne die Quelle zu lesen.
    pub mtime_ns: Option<i64>,
    pub lauf_id: Option<String>,
}

/// Phase eines Laufs. „Direkt“ kennt geplant → läuft → fertig/abgebrochen;
/// „Umschalten“ zusätzlich hochgeladen → umgeschaltet → fertig/zurückgerollt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaufPhase {
    Geplant,
    Laeuft,
    Fertig,
    Abgebrochen,
    /// Schattendateien liegen am Ziel – wartet auf „Umschalten“ (oder „Abbrechen“).
    Hochgeladen,
    /// Umgeschaltet, Sicherungen liegen am Ziel – wartet auf „Übernehmen“ oder „Rollback“.
    Umgeschaltet,
    Zurueckgerollt,
}

impl LaufPhase {
    pub fn ist_beendet(self) -> bool {
        matches!(self, LaufPhase::Fertig | LaufPhase::Abgebrochen | LaufPhase::Zurueckgerollt)
    }

    /// Wartet auf den nächsten Schritt – die Stufe ist so lange gesperrt.
    pub fn ist_offen(self) -> bool {
        matches!(self, LaufPhase::Hochgeladen | LaufPhase::Umgeschaltet)
    }
}

/// Status einer Operation im Journal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JournalStatus {
    Offen,
    Erledigt,
    Fehler,
    /// Nicht ausgeführt – Lauf abgebrochen oder eine Voraussetzung ist fehlgeschlagen.
    Uebersprungen,
    /// Umschalten: als Schattendatei hochgeladen (bzw. Ordner/Rechte erledigt).
    Hochgeladen,
    /// Umschalten: umgeschaltet, Sicherung liegt noch am Ziel.
    Umgeschaltet,
    /// Umschalten: zurückgenommen (Rollback oder Abbrechen).
    Zurueckgerollt,
}

/// Schwere eines Protokolleintrags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtokollEbene {
    Info,
    Warnung,
    Fehler,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Lauf {
    pub id: String,
    pub strecke_id: String,
    pub stufe_id: String,
    pub modus: Modus,
    pub phase: LaufPhase,
    pub gestartet: i64,
    pub beendet: Option<i64>,
    pub zusammenfassung: PlanZusammenfassung,
    /// Ergebnis der Ausführung (frei, JSON), sobald der Lauf beendet ist.
    pub ergebnis: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JournalZeile {
    pub nr: usize,
    pub op: OpArt,
    pub pfad: RelPath,
    pub quelle: Option<RelPath>,
    pub modus: Option<String>,
    pub groesse: u64,
    pub status: JournalStatus,
    pub fehler: Option<String>,
    /// Bemerkenswertes ohne Fehler, z. B. „Datenverbindung 1-mal mit neuem Port wiederholt“.
    pub hinweis: Option<String>,
    pub aktualisiert: i64,
    /// Umschalten: Hash und Zeit der Quelle der hochgeladenen Datei (nur intern).
    #[serde(skip)]
    pub hash: Option<Hash>,
    #[serde(skip)]
    pub mtime_ns: Option<i64>,
}

/// Neuer Protokolleintrag.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Protokollieren<'a> {
    pub zeit: i64,
    pub ebene: Option<ProtokollEbene>,
    pub strecke_id: Option<&'a str>,
    pub stufe_id: Option<&'a str>,
    pub lauf_id: Option<&'a str>,
    /// Kurzname der Aktion, z. B. `lauf_gestartet`, `operation_fehler`.
    pub aktion: &'a str,
    pub pfad: Option<&'a str>,
    pub details: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProtokollEintrag {
    pub id: i64,
    pub zeit: i64,
    pub ebene: ProtokollEbene,
    pub strecke_id: Option<String>,
    pub stufe_id: Option<String>,
    pub lauf_id: Option<String>,
    pub aktion: String,
    pub pfad: Option<String>,
    pub details: Option<Value>,
}

/// Auswahl beim Lesen des Protokolls. Neueste zuerst; `vor_id` blättert zurück.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct ProtokollFilter {
    pub strecke_id: Option<String>,
    pub stufe_id: Option<String>,
    pub lauf_id: Option<String>,
    /// Nur Einträge mit kleinerer Kennung (ältere).
    pub vor_id: Option<i64>,
    /// Höchstzahl; 0 = Standard (200).
    pub limit: usize,
}

/// Aktuelle Zeit in Millisekunden seit 1970.
pub fn jetzt_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

pub struct Datenbank {
    conn: Connection,
}

impl Datenbank {
    /// Öffnet (oder erstellt) `<ordner>/stellwerk.sqlite`; der Ordner wird bei Bedarf angelegt.
    pub fn im_ordner(ordner: &Path) -> Result<Self> {
        std::fs::create_dir_all(ordner).map_err(|e| StellwerkError::io(ordner, e))?;
        Self::oeffnen(&ordner.join(DATEINAME))
    }

    pub fn oeffnen(datei: &Path) -> Result<Self> {
        Self::verbinden(Connection::open(datei), Some(datei))
    }

    /// Nur für Tests und Vorschau: Datenbank im Speicher.
    pub fn im_speicher() -> Result<Self> {
        Self::verbinden(Connection::open_in_memory(), None)
    }

    /// Öffnet, stellt ein und legt beim ersten Mal das Schema an.
    fn verbinden(geoeffnet: rusqlite::Result<Connection>, datei: Option<&Path>) -> Result<Self> {
        let vorbereiten = move || -> rusqlite::Result<(Connection, i64)> {
            let conn = geoeffnet?;
            if datei.is_some() {
                // WAL: Lesen während eines Laufs, wenige Synchronisationen je Schreibvorgang
                conn.pragma_update(None, "journal_mode", "WAL")?;
            }
            conn.pragma_update(None, "synchronous", "NORMAL")?;
            conn.pragma_update(None, "foreign_keys", true)?;
            let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
            if version < SCHEMA_VERSION {
                // Fehlende Stufen nacheinander; vorhandene Daten bleiben erhalten
                let schritte = [SCHEMA, SCHEMA_2, SCHEMA_3, SCHEMA_4, SCHEMA_5];
                let fehlend = schritte[version.max(0) as usize..].concat();
                conn.execute_batch(&format!("BEGIN; {fehlend} PRAGMA user_version = {SCHEMA_VERSION}; COMMIT;"))?;
            }
            Ok((conn, version))
        };
        let (conn, version) = vorbereiten().map_err(|e| db_fehler(e, datei))?;
        if version > SCHEMA_VERSION {
            return Err(StellwerkError::Datenbank(format!(
                "Datenbank hat Schema {version}, diese Version kennt nur bis {SCHEMA_VERSION}{}",
                datei.map(|d| format!(" ({})", d.display())).unwrap_or_default()
            )));
        }
        Ok(Self { conn })
    }

    /// Legt einen Lauf (Phase `geplant`) samt Journal an – in einem Schritt.
    /// Liefert die Kennung (Zeit in Basis 36, bei Gleichstand die nächste freie).
    pub fn lauf_anlegen(&mut self, strecke_id: &str, stufe_id: &str, modus: Modus, plan: &Plan, zeit: i64) -> Result<String> {
        let tx = self.conn.transaction().map_err(db)?;
        let mut kandidat = zeit;
        let id = loop {
            let id = basis36(kandidat.max(0) as u64);
            let r = tx.execute(
                "INSERT INTO lauf (id, strecke_id, stufe_id, modus, phase, gestartet, zusammenfassung)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![id, strecke_id, stufe_id, text(&modus), text(&LaufPhase::Geplant), zeit, json(&plan.zusammenfassung)],
            );
            match r {
                Err(rusqlite::Error::SqliteFailure(e, _)) if e.code == ErrorCode::ConstraintViolation => kandidat += 1,
                r => {
                    r.map_err(db)?;
                    break id;
                }
            }
        };
        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO journal (lauf_id, nr, op, pfad, quelle, modus, groesse, status, aktualisiert)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                )
                .map_err(db)?;
            for o in &plan.operationen {
                stmt.execute(params![
                    id,
                    o.nr as i64,
                    text(&o.art),
                    o.pfad.as_str(),
                    o.quelle.as_ref().map(RelPath::as_str),
                    o.modus.map(|m| m.to_string()),
                    o.groesse as i64,
                    text(&JournalStatus::Offen),
                    zeit
                ])
                .map_err(db)?;
            }
        }
        tx.commit().map_err(db)?;
        Ok(id)
    }

    /// Setzt die Phase; bei `fertig`/`abgebrochen` auch Endzeit und Ergebnis.
    pub fn lauf_phase(&self, id: &str, phase: LaufPhase, zeit: i64, ergebnis: Option<&Value>) -> Result<()> {
        let beendet = phase.ist_beendet().then_some(zeit);
        let n = self
            .conn
            .execute(
                "UPDATE lauf SET phase = ?2, beendet = ?3, ergebnis = COALESCE(?4, ergebnis) WHERE id = ?1",
                params![id, text(&phase), beendet, ergebnis.map(Value::to_string)],
            )
            .map_err(db)?;
        gefunden(n, || format!("Lauf „{id}“"))
    }

    pub fn journal_setzen(&self, lauf_id: &str, nr: usize, status: JournalStatus, fehler: Option<&str>, hinweis: Option<&str>, zeit: i64) -> Result<()> {
        let n = self
            .conn
            .execute(
                "UPDATE journal SET status = ?3, fehler = ?4, hinweis = ?5, aktualisiert = ?6 WHERE lauf_id = ?1 AND nr = ?2",
                params![lauf_id, nr as i64, text(&status), fehler, hinweis, zeit],
            )
            .map_err(db)?;
        gefunden(n, || format!("Journal-Eintrag {nr} von Lauf „{lauf_id}“"))
    }

    /// Umschalten: Hash und Zeit der Quelle einer hochgeladenen Datei merken.
    pub fn journal_uebertragen(&self, lauf_id: &str, nr: usize, hash: Hash, mtime_ns: Option<i64>) -> Result<()> {
        let n = self
            .conn
            .execute(
                "UPDATE journal SET hash = ?3, mtime_ns = ?4 WHERE lauf_id = ?1 AND nr = ?2",
                params![lauf_id, nr as i64, hash.0.to_vec(), mtime_ns],
            )
            .map_err(db)?;
        gefunden(n, || format!("Journal-Eintrag {nr} von Lauf „{lauf_id}“"))
    }

    /// Der neueste Lauf der Stufe, der auf seinen nächsten Schritt wartet (Umschalten).
    pub fn offener_lauf(&self, strecke_id: &str, stufe_id: &str) -> Result<Option<Lauf>> {
        self.conn
            .query_row(
                &format!(
                    "SELECT {LAUF_SPALTEN} FROM lauf WHERE strecke_id = ?1 AND stufe_id = ?2 AND phase IN ('hochgeladen', 'umgeschaltet')
                     ORDER BY gestartet DESC, id DESC LIMIT 1"
                ),
                params![strecke_id, stufe_id],
                lauf_aus,
            )
            .optional()
            .map_err(db)
    }

    // --- Manifest -----------------------------------------------------------

    /// Sichert das Manifest einer Stufe für den Lauf (vor dem Umschalten).
    pub fn manifest_sichern(&mut self, strecke_id: &str, stufe_id: &str, lauf_id: &str) -> Result<()> {
        let tx = self.conn.transaction().map_err(db)?;
        tx.execute("DELETE FROM manifest_sicherung WHERE lauf_id = ?1", [lauf_id]).map_err(db)?;
        tx.execute(
            "INSERT INTO manifest_sicherung SELECT ?3, strecke_id, stufe_id, pfad, ist_ordner, groesse, hash, mtime_ns, lauf_id
             FROM manifest WHERE strecke_id = ?1 AND stufe_id = ?2",
            params![strecke_id, stufe_id, lauf_id],
        )
        .map_err(db)?;
        tx.commit().map_err(db)
    }

    /// Rollback: das gesicherte Manifest zurück; die Sicherung entfällt.
    pub fn manifest_zuruecksetzen(&mut self, strecke_id: &str, stufe_id: &str, lauf_id: &str) -> Result<()> {
        let tx = self.conn.transaction().map_err(db)?;
        tx.execute("DELETE FROM manifest WHERE strecke_id = ?1 AND stufe_id = ?2", params![strecke_id, stufe_id]).map_err(db)?;
        tx.execute(
            "INSERT INTO manifest SELECT strecke_id, stufe_id, pfad, ist_ordner, groesse, hash, mtime_ns, eintrag_lauf_id
             FROM manifest_sicherung WHERE lauf_id = ?1",
            [lauf_id],
        )
        .map_err(db)?;
        tx.execute("DELETE FROM manifest_sicherung WHERE lauf_id = ?1", [lauf_id]).map_err(db)?;
        tx.commit().map_err(db)
    }

    /// Übernommen: die Sicherung wird nicht mehr gebraucht.
    pub fn manifest_sicherung_verwerfen(&self, lauf_id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM manifest_sicherung WHERE lauf_id = ?1", [lauf_id]).map(drop).map_err(db)
    }

    /// Trägt einen Pfad ein oder ersetzt ihn.
    pub fn manifest_setzen(&self, strecke_id: &str, stufe_id: &str, e: &ManifestEintrag) -> Result<()> {
        manifest_einfuegen(&self.conn, strecke_id, stufe_id, e).map_err(db)
    }

    /// Entfernt einen Pfad samt allem darunter (gelöschter Ordner).
    pub fn manifest_entfernen(&self, strecke_id: &str, stufe_id: &str, pfad: &RelPath) -> Result<()> {
        // Exakter Präfix (LIKE wäre in SQLite unabhängig von Groß/Klein)
        let unter = format!("{}/", pfad.as_str());
        self.conn
            .execute(
                "DELETE FROM manifest WHERE strecke_id = ?1 AND stufe_id = ?2 AND (pfad = ?3 OR substr(pfad, 1, ?5) = ?4)",
                params![strecke_id, stufe_id, pfad.as_str(), unter, unter.chars().count() as i64],
            )
            .map(drop)
            .map_err(db)
    }

    /// Ersetzt das ganze Manifest einer Stufe (Erstvergleich, Vollabgleich) – in einem Schritt.
    pub fn manifest_ersetzen(&mut self, strecke_id: &str, stufe_id: &str, eintraege: &[ManifestEintrag]) -> Result<()> {
        let tx = self.conn.transaction().map_err(db)?;
        tx.execute("DELETE FROM manifest WHERE strecke_id = ?1 AND stufe_id = ?2", params![strecke_id, stufe_id]).map_err(db)?;
        for e in eintraege {
            manifest_einfuegen(&tx, strecke_id, stufe_id, e).map_err(db)?;
        }
        tx.commit().map_err(db)
    }

    /// Manifest einer Stufe, nach Pfad sortiert. Leer, wenn noch nie übertragen.
    pub fn manifest(&self, strecke_id: &str, stufe_id: &str) -> Result<Vec<ManifestEintrag>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT pfad, ist_ordner, groesse, hash, mtime_ns, lauf_id FROM manifest
                 WHERE strecke_id = ?1 AND stufe_id = ?2 ORDER BY pfad",
            )
            .map_err(db)?;
        let zeilen = stmt.query_map(params![strecke_id, stufe_id], manifest_aus).map_err(db)?;
        zeilen.collect::<rusqlite::Result<_>>().map_err(db)
    }

    // --- Stufen-Status (Gelerntes) ------------------------------------------

    pub fn status_setzen(&self, strecke_id: &str, stufe_id: &str, schluessel: &str, wert: &str, zeit: i64) -> Result<()> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO stufen_status (strecke_id, stufe_id, schluessel, wert, aktualisiert)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![strecke_id, stufe_id, schluessel, wert, zeit],
            )
            .map(drop)
            .map_err(db)
    }

    pub fn status(&self, strecke_id: &str, stufe_id: &str, schluessel: &str) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT wert FROM stufen_status WHERE strecke_id = ?1 AND stufe_id = ?2 AND schluessel = ?3",
                params![strecke_id, stufe_id, schluessel],
                |r| r.get(0),
            )
            .optional()
            .map_err(db)
    }

    pub fn lauf(&self, id: &str) -> Result<Option<Lauf>> {
        self.conn
            .query_row(&format!("SELECT {LAUF_SPALTEN} FROM lauf WHERE id = ?1"), [id], lauf_aus)
            .optional()
            .map_err(db)
    }

    /// Läufe einer Stufe, neueste zuerst.
    pub fn laeufe(&self, strecke_id: &str, stufe_id: &str, limit: usize) -> Result<Vec<Lauf>> {
        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT {LAUF_SPALTEN} FROM lauf WHERE strecke_id = ?1 AND stufe_id = ?2
                 ORDER BY gestartet DESC, id DESC LIMIT ?3"
            ))
            .map_err(db)?;
        let zeilen = stmt.query_map(params![strecke_id, stufe_id, limit as i64], lauf_aus).map_err(db)?;
        zeilen.collect::<rusqlite::Result<_>>().map_err(db)
    }

    /// Alle Operationen eines Laufs in Plan-Reihenfolge.
    pub fn journal(&self, lauf_id: &str) -> Result<Vec<JournalZeile>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT nr, op, pfad, quelle, modus, groesse, status, fehler, hinweis, aktualisiert, hash, mtime_ns
                 FROM journal WHERE lauf_id = ?1 ORDER BY nr",
            )
            .map_err(db)?;
        let zeilen = stmt
            .query_map([lauf_id], |r| {
                Ok(JournalZeile {
                    nr: r.get::<_, i64>(0)? as usize,
                    op: aus_text(r, 1)?,
                    pfad: pfad_aus(r, 2)?,
                    quelle: r.get::<_, Option<String>>(3)?.map(|s| pfad_aus_text(&s, 3)).transpose()?,
                    modus: r.get(4)?,
                    groesse: r.get::<_, i64>(5)? as u64,
                    status: aus_text(r, 6)?,
                    fehler: r.get(7)?,
                    hinweis: r.get(8)?,
                    aktualisiert: r.get(9)?,
                    hash: hash_aus(r, 10)?,
                    mtime_ns: r.get(11)?,
                })
            })
            .map_err(db)?;
        zeilen.collect::<rusqlite::Result<_>>().map_err(db)
    }

    /// Schreibt einen Protokolleintrag (Ebene ohne Angabe: `info`) und liefert seine Kennung.
    pub fn protokollieren(&self, e: &Protokollieren) -> Result<i64> {
        self.conn
            .execute(
                "INSERT INTO protokoll (zeit, ebene, strecke_id, stufe_id, lauf_id, aktion, pfad, details)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    e.zeit,
                    text(&e.ebene.unwrap_or(ProtokollEbene::Info)),
                    e.strecke_id,
                    e.stufe_id,
                    e.lauf_id,
                    e.aktion,
                    e.pfad,
                    e.details.as_ref().map(Value::to_string)
                ],
            )
            .map_err(db)?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Protokolleinträge, neueste zuerst.
    pub fn protokoll(&self, f: &ProtokollFilter) -> Result<Vec<ProtokollEintrag>> {
        let limit = if f.limit == 0 { 200 } else { f.limit };
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, zeit, ebene, strecke_id, stufe_id, lauf_id, aktion, pfad, details FROM protokoll
                 WHERE (?1 IS NULL OR strecke_id = ?1) AND (?2 IS NULL OR stufe_id = ?2)
                   AND (?3 IS NULL OR lauf_id = ?3) AND (?4 IS NULL OR id < ?4)
                 ORDER BY id DESC LIMIT ?5",
            )
            .map_err(db)?;
        let zeilen = stmt
            .query_map(params![f.strecke_id, f.stufe_id, f.lauf_id, f.vor_id, limit as i64], |r| {
                Ok(ProtokollEintrag {
                    id: r.get(0)?,
                    zeit: r.get(1)?,
                    ebene: aus_text(r, 2)?,
                    strecke_id: r.get(3)?,
                    stufe_id: r.get(4)?,
                    lauf_id: r.get(5)?,
                    aktion: r.get(6)?,
                    pfad: r.get(7)?,
                    details: r.get::<_, Option<String>>(8)?.map(|s| aus_json(&s, 8)).transpose()?,
                })
            })
            .map_err(db)?;
        zeilen.collect::<rusqlite::Result<_>>().map_err(db)
    }

    /// Nur für Tests: direkter Zugriff, z. B. um beschädigte Daten nachzubilden.
    #[doc(hidden)]
    pub fn verbindung(&self) -> &Connection {
        &self.conn
    }
}

const LAUF_SPALTEN: &str = "id, strecke_id, stufe_id, modus, phase, gestartet, beendet, zusammenfassung, ergebnis";

impl ManifestEintrag {
    /// Als Eintrag der Zielseite für den Vergleich (Hash und Zeit aus dem Manifest).
    pub fn als_eintrag(&self) -> Eintrag {
        Eintrag {
            pfad: self.pfad.clone(),
            art: if self.ist_ordner { EintragArt::Ordner } else { EintragArt::Datei },
            groesse: self.groesse,
            mtime_ns: self.mtime_ns,
            nur_online: false,
            hash: self.hash,
            warnungen: vec![],
        }
    }
}

fn manifest_einfuegen(conn: &Connection, strecke_id: &str, stufe_id: &str, e: &ManifestEintrag) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO manifest (strecke_id, stufe_id, pfad, ist_ordner, groesse, hash, mtime_ns, lauf_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![strecke_id, stufe_id, e.pfad.as_str(), e.ist_ordner, e.groesse as i64, e.hash.map(|h| h.0.to_vec()), e.mtime_ns, e.lauf_id],
    )
    .map(drop)
}

fn hash_aus(r: &Row, spalte: usize) -> rusqlite::Result<Option<Hash>> {
    let hash: Option<Vec<u8>> = r.get(spalte)?;
    hash.map(|h| <[u8; 32]>::try_from(h.as_slice()).map(Hash).map_err(|e| umwandlung(spalte, e))).transpose()
}

fn manifest_aus(r: &Row) -> rusqlite::Result<ManifestEintrag> {
    Ok(ManifestEintrag {
        pfad: pfad_aus(r, 0)?,
        ist_ordner: r.get(1)?,
        groesse: r.get::<_, i64>(2)? as u64,
        hash: hash_aus(r, 3)?,
        mtime_ns: r.get(4)?,
        lauf_id: r.get(5)?,
    })
}

fn lauf_aus(r: &Row) -> rusqlite::Result<Lauf> {
    Ok(Lauf {
        id: r.get(0)?,
        strecke_id: r.get(1)?,
        stufe_id: r.get(2)?,
        modus: aus_text(r, 3)?,
        phase: aus_text(r, 4)?,
        gestartet: r.get(5)?,
        beendet: r.get(6)?,
        zusammenfassung: aus_json(&r.get::<_, String>(7)?, 7)?,
        ergebnis: r.get::<_, Option<String>>(8)?.map(|s| aus_json(&s, 8)).transpose()?,
    })
}

/// Name eines Aufzählungswerts wie in JSON (`snake_case`).
fn text<T: Serialize>(t: &T) -> String {
    serde_json::to_value(t).ok().and_then(|v| v.as_str().map(String::from)).expect("Aufzählungswert als Text")
}

fn json<T: Serialize>(t: &T) -> String {
    serde_json::to_string(t).expect("Zusammenfassung ist immer als JSON darstellbar")
}

fn aus_text<T: DeserializeOwned>(r: &Row, spalte: usize) -> rusqlite::Result<T> {
    aus_json_wert(Value::String(r.get(spalte)?), spalte)
}

fn aus_json<T: DeserializeOwned>(s: &str, spalte: usize) -> rusqlite::Result<T> {
    aus_json_wert(serde_json::from_str(s).map_err(json_fehler(spalte))?, spalte)
}

fn aus_json_wert<T: DeserializeOwned>(v: Value, spalte: usize) -> rusqlite::Result<T> {
    serde_json::from_value(v).map_err(json_fehler(spalte))
}

/// Nicht generisch – sonst zählte jede Typ-Ausprägung als eigener, ungeprüfter Zweig.
fn json_fehler(spalte: usize) -> impl Fn(serde_json::Error) -> rusqlite::Error {
    move |e| umwandlung(spalte, e)
}

fn pfad_aus(r: &Row, spalte: usize) -> rusqlite::Result<RelPath> {
    pfad_aus_text(&r.get::<_, String>(spalte)?, spalte)
}

fn pfad_aus_text(s: &str, spalte: usize) -> rusqlite::Result<RelPath> {
    RelPath::new(s).map_err(|e| umwandlung(spalte, e))
}

fn umwandlung(spalte: usize, e: impl std::error::Error + Send + Sync + 'static) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(spalte, rusqlite::types::Type::Text, Box::new(e))
}

fn gefunden(n: usize, was: impl FnOnce() -> String) -> Result<()> {
    if n == 0 {
        Err(StellwerkError::NichtGefunden(was()))
    } else {
        Ok(())
    }
}

fn db(e: rusqlite::Error) -> StellwerkError {
    db_fehler(e, None)
}

fn db_fehler(e: rusqlite::Error, datei: Option<&Path>) -> StellwerkError {
    StellwerkError::Datenbank(match datei {
        Some(d) => format!("{e} ({})", d.display()),
        None => e.to_string(),
    })
}

/// Zahl in Basis 36 (Ziffern und Kleinbuchstaben) – kurze, sortierbare Lauf-Kennung.
pub fn basis36(mut n: u64) -> String {
    const Z: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut s = vec![Z[(n % 36) as usize]];
    while n >= 36 {
        n /= 36;
        s.push(Z[(n % 36) as usize]);
    }
    s.reverse();
    String::from_utf8(s).expect("nur ASCII")
}
