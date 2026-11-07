//! Ausführung „Direkt“ in ein lokales Ziel (ARCHITEKTUR Kap. 8, KONZEPT Kap. 7.1).
//!
//! - **Worker-Pool:** *n* Threads führen Operationen aus; *n* = 1 ist kein eigener
//!   Codepfad. Ein Koordinator gibt nur Operationen frei, deren Voraussetzungen
//!   (`nach`) erledigt sind, und schreibt Journal und Fortschritt – Journal und
//!   Datenbank bleiben so in einem Thread.
//! - **Nur das Ziel wird beschrieben.** Die Quelle wird ausschließlich gelesen (E3).
//! - **Nie halbe Dateien:** Inhalt wird als `.<name>.stw-<lauf>` im Zielordner
//!   geschrieben, auf den Datenträger gebracht und erst dann umbenannt.
//! - **Änderungszeit der Quelle** wird übernommen – der nächste Vergleich erkennt
//!   die Datei dann ohne Lesen des Inhalts als gleich.
//! - **Dateirechte:** neue Dateien erhalten die Rechte der Quelle, überschriebene
//!   behalten die bisherigen Rechte am Ziel. Rechte der Stufe kommen als eigene
//!   Operation aus dem Plan.
//! - **Fehler je Datei** werden gesammelt, der Lauf läuft weiter – außer mit
//!   „bei Fehler stoppen“. Was von einer fehlgeschlagenen Operation abhängt, wird
//!   übersprungen.
//! - **Abbruch** kooperativ: Es wird nichts Neues begonnen, laufende Kopien brechen
//!   ab und räumen ihre Temporärdatei weg.
//! - Gelöscht wird nur, was der Plan nennt; Ordner nur, wenn sie leer sind.

use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Mutex};
use std::time::{Duration, Instant};

use filetime::FileTime;
use serde::Serialize;

use crate::error::{Result, StellwerkError};
use crate::model::RelPath;
use crate::plan::{OpArt, Operation, Plan};
use crate::store::JournalStatus;

/// Höchstzahl paralleler Worker lokal (ARCHITEKTUR Kap. 8.1).
pub const MAX_WORKER: usize = 8;

/// Größe eines Kopierblocks; zwischen zwei Blöcken wird der Abbruch geprüft.
const BLOCK: usize = 1 << 20;

/// So viele Worker, wie sicher betreibbar: höchstens 8, nicht mehr als Prozessorkerne.
pub fn standard_worker() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get()).min(MAX_WORKER)
}

#[derive(Debug, Clone)]
pub struct Optionen {
    /// Anzahl Worker (mindestens 1).
    pub worker: usize,
    /// Nach dem ersten Fehler nichts Neues mehr beginnen.
    pub bei_fehler_stoppen: bool,
    /// Mindestabstand zwischen zwei Fortschrittsmeldungen (die letzte kommt immer).
    pub fortschritt_abstand: Duration,
}

/// Fortschritt eines Laufs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ExecFortschritt {
    /// Abgeschlossene Operationen (erledigt oder fehlgeschlagen).
    pub fertig: usize,
    pub gesamt: usize,
    pub bytes: u64,
    pub bytes_gesamt: u64,
    pub fehler: usize,
}

/// Eine fehlgeschlagene Operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DateiFehler {
    pub nr: usize,
    pub art: OpArt,
    pub pfad: RelPath,
    pub meldung: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct LaufErgebnis {
    pub erledigt: usize,
    pub fehler: Vec<DateiFehler>,
    pub uebersprungen: usize,
    /// Vom Nutzer abgebrochen.
    pub abgebrochen: bool,
    /// Wegen „bei Fehler stoppen“ vorzeitig beendet.
    pub gestoppt: bool,
    /// Übertragene Bytes.
    pub bytes: u64,
    /// Das Journal konnte nicht geschrieben werden – der Lauf wurde deshalb beendet.
    pub journal_fehler: Option<String>,
}

impl LaufErgebnis {
    /// Alles erledigt, nichts fehlgeschlagen, nichts übersprungen.
    pub fn vollstaendig(&self) -> bool {
        self.fehler.is_empty() && self.uebersprungen == 0 && !self.abgebrochen && self.journal_fehler.is_none()
    }
}

/// Rückmeldung an das Journal: Operation, neuer Status, ggf. Meldung.
pub type JournalMelden<'a> = dyn FnMut(&Operation, JournalStatus, Option<&str>) -> Result<()> + 'a;

/// Führt einen Plan im Modus „Direkt“ aus: liest aus `quelle`, schreibt nach `ziel`.
/// `lauf_id` macht die Namen der Temporärdateien eindeutig.
#[allow(clippy::too_many_arguments)]
pub fn direkt_ausfuehren(
    plan: &Plan,
    quelle: &Path,
    ziel: &Path,
    lauf_id: &str,
    opt: &Optionen,
    abbruch: &AtomicBool,
    journal: &mut JournalMelden,
    fortschritt: &mut dyn FnMut(ExecFortschritt),
) -> LaufErgebnis {
    let ops = &plan.operationen;
    let n = ops.len();
    let worker = opt.worker.max(1);
    let mut offen_vor: Vec<usize> = ops.iter().map(|o| o.nach.len()).collect();
    let mut nachfolger: Vec<Vec<usize>> = vec![Vec::new(); n];
    for o in ops {
        for &v in &o.nach {
            nachfolger[v].push(o.nr);
        }
    }
    let mut bereit: VecDeque<usize> = (0..n).filter(|&i| offen_vor[i] == 0).collect();
    let mut status: Vec<Option<JournalStatus>> = vec![None; n];
    let mut erg = LaufErgebnis::default();
    let mut stand = ExecFortschritt {
        fertig: 0,
        gesamt: n,
        bytes: 0,
        bytes_gesamt: plan.zusammenfassung.bytes,
        fehler: 0,
    };
    fortschritt(stand);
    let mut zuletzt = Instant::now();
    let ziele = Wurzeln { quelle, ziel, lauf_id };

    let (auftrag_tx, auftrag_rx) = mpsc::channel::<usize>();
    let auftrag_rx = Mutex::new(auftrag_rx);
    let (fertig_tx, fertig_rx) = mpsc::channel::<(usize, Result<u64>)>();
    std::thread::scope(|s| {
        for _ in 0..worker {
            let (rx, tx, ziele) = (&auftrag_rx, fertig_tx.clone(), &ziele);
            s.spawn(move || {
                // Endet, sobald der Koordinator den Sender schließt
                while let Ok(nr) = rx.lock().expect("Worker-Warteschlange").recv() {
                    let _ = tx.send((nr, ausfuehren(&ops[nr], ziele, abbruch)));
                }
            });
        }

        let mut laufend = 0;
        loop {
            if abbruch.load(Ordering::Relaxed) {
                erg.abgebrochen = true;
            }
            let anhalten = erg.abgebrochen || erg.gestoppt || erg.journal_fehler.is_some();
            while laufend < worker && !anhalten {
                let Some(nr) = bereit.pop_front() else { break };
                auftrag_tx.send(nr).expect("Worker laufen, solange der Koordinator läuft");
                laufend += 1;
            }
            if laufend == 0 {
                break;
            }
            let (nr, r) = fertig_rx.recv().expect("Ein Worker meldet jeden Auftrag zurück");
            laufend -= 1;
            let o = &ops[nr];
            let (st, meldung) = match r {
                Ok(bytes) => {
                    erg.erledigt += 1;
                    erg.bytes += bytes;
                    stand.bytes += bytes;
                    for &f in &nachfolger[nr] {
                        offen_vor[f] -= 1;
                        if offen_vor[f] == 0 {
                            bereit.push_back(f);
                        }
                    }
                    (JournalStatus::Erledigt, None)
                }
                Err(StellwerkError::Abbruch) => {
                    erg.abgebrochen = true;
                    erg.uebersprungen += 1;
                    (JournalStatus::Uebersprungen, Some("Lauf abgebrochen".to_string()))
                }
                Err(e) => {
                    let meldung = e.to_string();
                    erg.fehler.push(DateiFehler { nr, art: o.art, pfad: o.pfad.clone(), meldung: meldung.clone() });
                    erg.gestoppt |= opt.bei_fehler_stoppen;
                    stand.fehler += 1;
                    (JournalStatus::Fehler, Some(meldung))
                }
            };
            status[nr] = Some(st);
            if st != JournalStatus::Uebersprungen {
                stand.fertig += 1;
            }
            if let Err(e) = journal(o, st, meldung.as_deref()) {
                erg.journal_fehler.get_or_insert(e.to_string());
            }
            if zuletzt.elapsed() >= opt.fortschritt_abstand {
                fortschritt(stand);
                zuletzt = Instant::now();
            }
        }
        drop(auftrag_tx);
    });

    // Was nicht ausgeführt wurde, wird mit Grund als übersprungen eingetragen
    for o in ops {
        if status[o.nr].is_some() {
            continue;
        }
        let voraussetzung = o.nach.iter().any(|&v| status[v] != Some(JournalStatus::Erledigt));
        let grund = if voraussetzung {
            "Voraussetzung nicht erfüllt"
        } else if erg.abgebrochen {
            "Lauf abgebrochen"
        } else {
            "Lauf vorzeitig beendet"
        };
        status[o.nr] = Some(JournalStatus::Uebersprungen);
        erg.uebersprungen += 1;
        if let Err(e) = journal(o, JournalStatus::Uebersprungen, Some(grund)) {
            erg.journal_fehler.get_or_insert(e.to_string());
        }
    }
    fortschritt(stand);
    erg
}

struct Wurzeln<'a> {
    quelle: &'a Path,
    ziel: &'a Path,
    lauf_id: &'a str,
}

/// Führt eine Operation aus und liefert die übertragenen Bytes.
fn ausfuehren(o: &Operation, w: &Wurzeln, abbruch: &AtomicBool) -> Result<u64> {
    let z = w.ziel.join(o.pfad.as_str());
    match o.art {
        OpArt::OrdnerAnlegen => ordner_anlegen(&z).map(|()| 0),
        OpArt::Kopieren | OpArt::Ueberschreiben => {
            let q = w.quelle.join(o.quelle.as_ref().expect("Kopieren hat immer eine Quelle").as_str());
            datei_schreiben(&q, &z, o.art == OpArt::Kopieren, w.lauf_id, abbruch)
        }
        OpArt::RechteSetzen => rechte_setzen(&z, o.modus.expect("Rechte setzen hat immer einen Modus").bits()).map(|()| 0),
        OpArt::DateiLoeschen => datei_loeschen(&z).map(|()| 0),
        OpArt::OrdnerLoeschen => ordner_loeschen(&z).map(|()| 0),
    }
}

fn io(p: &Path) -> impl FnOnce(std::io::Error) -> StellwerkError + '_ {
    move |e| StellwerkError::io(p, e)
}

/// Legt einen Ordner an; ist er inzwischen da, ist das in Ordnung.
fn ordner_anlegen(z: &Path) -> Result<()> {
    match fs::create_dir(z) {
        Err(e) if e.kind() == ErrorKind::AlreadyExists && ist_echter_ordner(z) => Ok(()),
        r => r.map_err(io(z)),
    }
}

fn ist_echter_ordner(p: &Path) -> bool {
    fs::symlink_metadata(p).is_ok_and(|m| m.is_dir())
}

/// Schreibt `q` nach `z` über eine Temporärdatei im Zielordner.
fn datei_schreiben(q: &Path, z: &Path, neu: bool, lauf_id: &str, abbruch: &AtomicBool) -> Result<u64> {
    let bisher = fs::symlink_metadata(z).ok();
    if neu && bisher.is_some() {
        return Err(StellwerkError::Vorhanden(format!("{} – am Ziel inzwischen vorhanden, bitte neu vergleichen", z.display())));
    }
    let mut ein = File::open(q).map_err(io(q))?;
    let meta = ein.metadata().map_err(io(q))?;
    if !meta.is_file() {
        return Err(StellwerkError::NichtUnterstuetzt(format!("{} ist keine Datei", q.display())));
    }
    let name = z.file_name().expect("Zielpfad endet auf einen Namen").to_string_lossy();
    let tmp: PathBuf = z.with_file_name(format!(".{name}.stw-{lauf_id}"));
    let mut aus = OpenOptions::new().write(true).create_new(true).open(&tmp).map_err(io(&tmp))?;

    let ergebnis = (|| {
        let mut puffer = vec![0u8; BLOCK];
        let mut bytes = 0u64;
        loop {
            if abbruch.load(Ordering::Relaxed) {
                return Err(StellwerkError::Abbruch);
            }
            let k = ein.read(&mut puffer).map_err(io(q))?;
            if k == 0 {
                break;
            }
            aus.write_all(&puffer[..k]).map_err(io(&tmp))?;
            bytes += k as u64;
        }
        aus.sync_all().map_err(io(&tmp))?;
        // Neu: Rechte der Quelle; überschrieben: bisherige Rechte am Ziel
        let rechte = bisher.as_ref().filter(|m| m.is_file()).map_or_else(|| meta.permissions(), |m| m.permissions());
        fs::set_permissions(&tmp, rechte).map_err(io(&tmp))?;
        filetime::set_file_mtime(&tmp, FileTime::from_last_modification_time(&meta)).map_err(io(&tmp))?;
        fs::rename(&tmp, z).map_err(io(z))?;
        Ok(bytes)
    })();
    if ergebnis.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    ergebnis
}

#[cfg(unix)]
fn rechte_setzen(z: &Path, bits: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(z, fs::Permissions::from_mode(bits)).map_err(io(z))
}

/// Löscht eine Datei (auch einen Link, nie einen Ordner). Schon weg ist in Ordnung.
fn datei_loeschen(z: &Path) -> Result<()> {
    match fs::symlink_metadata(z) {
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
        Ok(m) if m.is_dir() => {
            Err(StellwerkError::NichtUnterstuetzt(format!("{} ist inzwischen ein Ordner – bitte neu vergleichen", z.display())))
        }
        _ => fs::remove_file(z).map_err(io(z)),
    }
}

/// Löscht einen Ordner nur, wenn er leer ist. Schon weg ist in Ordnung.
fn ordner_loeschen(z: &Path) -> Result<()> {
    match fs::remove_dir(z) {
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
        r => r.map_err(io(z)),
    }
}
