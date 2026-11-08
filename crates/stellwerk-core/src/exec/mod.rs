//! Ausführung „Direkt“ über einen Ziel-Endpunkt (ARCHITEKTUR Kap. 8, KONZEPT Kap. 7.1).
//!
//! - **Worker-Pool:** *n* Threads führen Operationen aus; *n* = 1 ist kein eigener
//!   Codepfad. Ein Koordinator gibt nur Operationen frei, deren Voraussetzungen
//!   (`nach`) erledigt sind, und schreibt Journal und Fortschritt – Journal und
//!   Datenbank bleiben so in einem Thread.
//! - **Nur das Ziel wird beschrieben.** Die Quelle kommt nur als [`QuellEndpunkt`]
//!   (lesend) herein (E3, A4); geschrieben wird über je eine Verbindung pro Worker.
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

pub mod umschalten;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::endpoint::{Geschrieben, QuellEndpunkt, ZielEndpunkt, ZielVerbindung};
use crate::scan::zeit_ns;
use crate::error::{Result, StellwerkError};
use crate::model::RelPath;
use crate::plan::{OpArt, Operation, Plan};
use crate::store::JournalStatus;

/// Höchstzahl paralleler Worker lokal (ARCHITEKTUR Kap. 8.1).
pub const MAX_WORKER: usize = 8;

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

/// Eine geschriebene Datei: Bytes und Hash des Inhalts, Änderungszeit der Quelle beim
/// Lesen – Grundlage für das Manifest entfernter Ziele.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Uebertragen {
    pub geschrieben: Geschrieben,
    pub quelle_mtime_ns: Option<i64>,
}

/// Rückmeldung an das Journal: Operation, neuer Status, ggf. Meldung, – bei
/// erledigten Dateien – was übertragen wurde und ein Hinweis der Verbindung
/// (z. B. wiederholte Datenverbindung).
pub type JournalMelden<'a> = dyn FnMut(&Operation, JournalStatus, Option<&str>, Option<&Uebertragen>, Option<&str>) -> Result<()> + 'a;

/// Führt einen Plan im Modus „Direkt“ aus: liest aus `quelle`, schreibt nach `ziel`.
/// `lauf_id` macht die Namen der Temporärdateien eindeutig. Jeder Worker öffnet
/// bei seinem ersten Auftrag eine eigene Verbindung zum Ziel; scheitert das, gilt
/// der Auftrag als fehlgeschlagen und der nächste versucht es erneut.
#[allow(clippy::too_many_arguments)]
pub fn direkt_ausfuehren(
    plan: &Plan,
    quelle: &dyn QuellEndpunkt,
    ziel: &dyn ZielEndpunkt,
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

    let (auftrag_tx, auftrag_rx) = mpsc::channel::<usize>();
    let auftrag_rx = Mutex::new(auftrag_rx);
    let (fertig_tx, fertig_rx) = mpsc::channel::<(usize, Result<Option<Uebertragen>>, Option<String>)>();
    std::thread::scope(|s| {
        for _ in 0..worker {
            let (rx, tx) = (&auftrag_rx, fertig_tx.clone());
            s.spawn(move || {
                let mut verbindung: Option<Box<dyn ZielVerbindung + '_>> = None;
                // Endet, sobald der Koordinator den Sender schließt
                while let Ok(nr) = rx.lock().expect("Worker-Warteschlange").recv() {
                    let r = match verbindung.as_mut() {
                        Some(v) => Ok(v),
                        None => ziel.verbinden().map(|v| verbindung.insert(v)),
                    }
                    .and_then(|v| ausfuehren(&ops[nr], quelle, v.as_mut(), lauf_id, abbruch));
                    let hinweis = verbindung.as_mut().and_then(|v| v.hinweis());
                    let _ = tx.send((nr, r, hinweis));
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
            let (nr, r, hinweis) = fertig_rx.recv().expect("Ein Worker meldet jeden Auftrag zurück");
            laufend -= 1;
            let o = &ops[nr];
            let mut uebertragen = None;
            let (st, meldung) = match r {
                Ok(u) => {
                    uebertragen = u;
                    let bytes = u.map_or(0, |u| u.geschrieben.bytes);
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
            if let Err(e) = journal(o, st, meldung.as_deref(), uebertragen.as_ref(), hinweis.as_deref()) {
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
        if let Err(e) = journal(o, JournalStatus::Uebersprungen, Some(grund), None, None) {
            erg.journal_fehler.get_or_insert(e.to_string());
        }
    }
    fortschritt(stand);
    erg
}

/// Führt eine Operation über die Verbindung des Workers aus; bei Dateien mit dem,
/// was übertragen wurde.
fn ausfuehren(o: &Operation, quelle: &dyn QuellEndpunkt, v: &mut dyn ZielVerbindung, lauf_id: &str, abbruch: &AtomicBool) -> Result<Option<Uebertragen>> {
    match o.art {
        OpArt::OrdnerAnlegen => v.ordner_anlegen(&o.pfad).map(|()| None),
        OpArt::Kopieren | OpArt::Ueberschreiben => {
            let q = quelle.oeffnen(o.quelle.as_ref().expect("Kopieren hat immer eine Quelle"))?;
            let quelle_mtime_ns = zeit_ns(q.geaendert);
            let geschrieben = v.datei_schreiben(&o.pfad, q, o.art == OpArt::Kopieren, lauf_id, abbruch)?;
            Ok(Some(Uebertragen { geschrieben, quelle_mtime_ns }))
        }
        OpArt::RechteSetzen => v.rechte_setzen(&o.pfad, o.modus.expect("Rechte setzen hat immer einen Modus").bits()).map(|()| None),
        OpArt::DateiLoeschen => v.datei_loeschen(&o.pfad).map(|()| None),
        OpArt::OrdnerLoeschen => v.ordner_loeschen(&o.pfad).map(|()| None),
    }
}
