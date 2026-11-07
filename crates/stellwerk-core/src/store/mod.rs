//! Laufzeitdaten in SQLite (ARCHITEKTUR Kap. 4.2): Läufe, Journal, Protokoll.
//!
//! - **Lauf:** eine Ausführung einer Stufe mit Phase und Zusammenfassung.
//! - **Journal:** jede geplante Operation eines Laufs. Sie wird vor dem Start mit
//!   Status `offen` angelegt und danach aktualisiert (`erledigt`, `fehler`,
//!   `uebersprungen`) – so ist jede Veränderung am Ziel nachvollziehbar.
//! - **Protokoll:** jede Aktion auf Laufwerken und Servern (E8) – Beginn, Fehler und Ende
//!   der Läufe. Einstellungen in Stellwerk selbst werden bewusst nicht protokolliert.
//!
//! `manifest` und `stufen_status` folgen mit FTPS (v0.4).
//!
//! Zeiten in Millisekunden seit 1970; sie werden übergeben, damit Tests sie festlegen können.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, ErrorCode, OptionalExtension, Row};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{Result, StellwerkError};
use crate::model::{Modus, RelPath};
use crate::plan::{OpArt, Plan, PlanZusammenfassung};

/// Dateiname im App-Ordner (neben `config.toml`).
pub const DATEINAME: &str = "stellwerk.sqlite";

/// Stand des Schemas (`PRAGMA user_version`).
const SCHEMA_VERSION: i64 = 1;

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

/// Phase eines Laufs. „Direkt“ kennt geplant → läuft → fertig/abgebrochen;
/// die Phasen von „Umschalten“ kommen mit v0.5 hinzu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaufPhase {
    Geplant,
    Laeuft,
    Fertig,
    Abgebrochen,
}

impl LaufPhase {
    pub fn ist_beendet(self) -> bool {
        matches!(self, LaufPhase::Fertig | LaufPhase::Abgebrochen)
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
    pub aktualisiert: i64,
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
            if version == 0 {
                conn.execute_batch(&format!("BEGIN; {SCHEMA} PRAGMA user_version = {SCHEMA_VERSION}; COMMIT;"))?;
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

    pub fn journal_setzen(&self, lauf_id: &str, nr: usize, status: JournalStatus, fehler: Option<&str>, zeit: i64) -> Result<()> {
        let n = self
            .conn
            .execute(
                "UPDATE journal SET status = ?3, fehler = ?4, aktualisiert = ?5 WHERE lauf_id = ?1 AND nr = ?2",
                params![lauf_id, nr as i64, text(&status), fehler, zeit],
            )
            .map_err(db)?;
        gefunden(n, || format!("Journal-Eintrag {nr} von Lauf „{lauf_id}“"))
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
                "SELECT nr, op, pfad, quelle, modus, groesse, status, fehler, aktualisiert
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
                    aktualisiert: r.get(8)?,
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
