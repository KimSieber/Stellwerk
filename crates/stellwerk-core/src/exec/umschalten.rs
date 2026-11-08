//! Modus „Umschalten“ (KONZEPT Kap. 7.2, Entscheidungen v0.5).
//!
//! Drei Phasen, jede für sich aufrufbar – „automatisch“ ruft sie nacheinander auf,
//! „manuell“ einzeln per Knopf:
//!
//! 1. **Hochladen** ([`hochladen`]): Neue und geänderte Dateien entstehen als
//!    Schattendatei `name.dpl-<lauf>` (Anhang hinten, damit der Server kein halbes
//!    Skript ausführt), wie bei „Direkt“ erst als Temporärdatei und dann umbenannt.
//!    Neue Ordner und Rechte entstehen gleich; Rechte von Dateien an der
//!    Schattendatei. Gelöscht wird nichts. Die Live-Seite bleibt unverändert.
//! 2. **Umschalten** ([`umschalten`]): nur Umbenennungen – Original →
//!    `name.bak-<lauf>`, Schattendatei → Original; zu löschende Dateien →
//!    `name.bak-<lauf>`. Stoppt beim ersten Fehler; die `.bak`-Dateien bleiben.
//! 3. **Übernehmen** ([`uebernehmen`]): `.bak`-Dateien und zu löschende Ordner
//!    entfernen – oder **Rollback** ([`zurueckrollen`]): alles zurück, auch die
//!    Schattendateien und neu angelegten Ordner.
//!
//! **Abbrechen** vor dem Umschalten ([`abbrechen`]) entfernt Schattendateien und
//! neue Ordner – die Live-Seite war nie berührt.
//!
//! Die Phasen 2 und 3 arbeiten der Reihe nach über eine Verbindung (nur
//! Umbenennen und Löschen, das geht schnell). Jede Aktion ist wiederholbar: Fehlt
//! die Datei, die umbenannt werden soll, ist dieser Schritt schon geschehen.
//! Vorgezogene Löschungen (Schreibweise `bild.jpg` → `Bild.jpg`) weichen beim
//! Umschalten zuerst – sie stehen im Plan vorn.
//!
//! Rechte überschriebener Dateien: lokal erhält die Schattendatei die Rechte der
//! Quelle (bei „Direkt“ behält die Datei ihre bisherigen Rechte).

use std::collections::HashSet;
use std::sync::atomic::AtomicBool;

use super::{direkt_ausfuehren, ExecFortschritt, JournalMelden, LaufErgebnis, Optionen};
use crate::endpoint::{Geschrieben, QuellDatei, QuellEndpunkt, ZielEndpunkt, ZielVerbindung};
use crate::error::{Result, StellwerkError};
use crate::model::RelPath;
use crate::plan::{OpArt, Operation, Plan};
use crate::store::{JournalStatus, JournalZeile};

/// Schattendatei im selben Ordner: `name.dpl-<lauf>`.
pub fn schatten_pfad(p: &RelPath, lauf_id: &str) -> RelPath {
    mit_anhang(p, "dpl", lauf_id)
}

/// Sicherung des Originals: `name.bak-<lauf>`.
pub fn backup_pfad(p: &RelPath, lauf_id: &str) -> RelPath {
    mit_anhang(p, "bak", lauf_id)
}

fn mit_anhang(p: &RelPath, art: &str, lauf_id: &str) -> RelPath {
    RelPath::new(&format!("{}.{art}-{lauf_id}", p.as_str())).expect("Anhang an einen gültigen Pfad bleibt gültig")
}

fn schreibt(art: OpArt) -> bool {
    matches!(art, OpArt::Kopieren | OpArt::Ueberschreiben)
}

fn loescht(art: OpArt) -> bool {
    matches!(art, OpArt::DateiLoeschen | OpArt::OrdnerLoeschen)
}

/// Der Plan eines Laufs, wie ihn das Journal festhält – für die späteren Phasen
/// (auch nach einem Neustart). Abhängigkeiten braucht es dort nicht: Umschalten,
/// Übernehmen und Rollback arbeiten der Reihe nach.
pub fn plan_aus_journal(zeilen: &[JournalZeile]) -> Plan {
    Plan {
        operationen: zeilen
            .iter()
            .map(|z| Operation { nr: z.nr, art: z.op, pfad: z.pfad.clone(), quelle: z.quelle.clone(), groesse: z.groesse, modus: None, nach: vec![] })
            .collect(),
        zusammenfassung: Default::default(),
        loeschliste: vec![],
        warnungen: vec![],
    }
}

// --- Phase 1: Hochladen ---------------------------------------------------------

/// Ziel, das Dateien als Schattendatei schreibt (Phase 1). `dateien`: Pfade, die
/// der Plan schreibt – deren Rechte gelten der Schattendatei.
pub struct SchattenZiel<'a> {
    pub innen: &'a dyn ZielEndpunkt,
    pub dateien: HashSet<RelPath>,
    pub lauf_id: &'a str,
}

impl ZielEndpunkt for SchattenZiel<'_> {
    fn verbinden(&self) -> Result<Box<dyn ZielVerbindung + '_>> {
        Ok(Box::new(SchattenVerbindung { ziel: self, innen: self.innen.verbinden()? }))
    }
}

struct SchattenVerbindung<'a> {
    ziel: &'a SchattenZiel<'a>,
    innen: Box<dyn ZielVerbindung + 'a>,
}

impl SchattenVerbindung<'_> {
    /// Schattendatei für geschriebene Dateien, sonst der Pfad selbst.
    fn ort(&self, p: &RelPath) -> RelPath {
        if self.ziel.dateien.contains(p) { schatten_pfad(p, self.ziel.lauf_id) } else { p.clone() }
    }
}

impl ZielVerbindung for SchattenVerbindung<'_> {
    fn ordner_anlegen(&mut self, p: &RelPath) -> Result<()> {
        self.innen.ordner_anlegen(p)
    }
    /// Immer als neue Datei – die Schattendatei darf es noch nicht geben.
    fn datei_schreiben(&mut self, p: &RelPath, q: QuellDatei, _neu: bool, lauf_id: &str, abbruch: &AtomicBool) -> Result<Geschrieben> {
        let ort = schatten_pfad(p, self.ziel.lauf_id);
        self.innen.datei_schreiben(&ort, q, true, lauf_id, abbruch)
    }
    fn rechte_setzen(&mut self, p: &RelPath, bits: u32) -> Result<()> {
        let ort = self.ort(p);
        self.innen.rechte_setzen(&ort, bits)
    }
    fn datei_loeschen(&mut self, p: &RelPath) -> Result<()> {
        let ort = self.ort(p);
        self.innen.datei_loeschen(&ort)
    }
    fn ordner_loeschen(&mut self, p: &RelPath) -> Result<()> {
        self.innen.ordner_loeschen(p)
    }
    fn umbenennen(&mut self, von: &RelPath, nach: &RelPath, ersetzen: bool) -> Result<()> {
        self.innen.umbenennen(von, nach, ersetzen)
    }
    fn ist_datei(&mut self, p: &RelPath) -> Result<bool> {
        let ort = self.ort(p);
        self.innen.ist_datei(&ort)
    }
    fn hinweis(&mut self) -> Option<String> {
        self.innen.hinweis()
    }
}

/// Phase 1: alles außer Löschungen ausführen, Dateien als Schattendatei. Das
/// Journal erhält die Operationen des ganzen Plans (gleiche Nummern); Löschungen
/// kommen hier nicht vor. Mit Worker-Pool wie „Direkt“.
#[allow(clippy::too_many_arguments)]
pub fn hochladen(
    plan: &Plan,
    quelle: &dyn QuellEndpunkt,
    ziel: &dyn ZielEndpunkt,
    lauf_id: &str,
    opt: &Optionen,
    abbruch: &AtomicBool,
    journal: &mut JournalMelden,
    fortschritt: &mut dyn FnMut(ExecFortschritt),
) -> LaufErgebnis {
    // Teilplan ohne Löschungen, neu nummeriert; `herkunft` führt zurück
    let herkunft: Vec<usize> = plan.operationen.iter().filter(|o| !loescht(o.art)).map(|o| o.nr).collect();
    let neu_nr = |alt: usize| herkunft.iter().position(|&h| h == alt);
    let teil = Plan {
        operationen: herkunft
            .iter()
            .enumerate()
            .map(|(i, &h)| {
                let o = &plan.operationen[h];
                Operation { nr: i, nach: o.nach.iter().filter_map(|&v| neu_nr(v)).collect(), ..o.clone() }
            })
            .collect(),
        ..plan.clone()
    };
    let dateien = plan.operationen.iter().filter(|o| schreibt(o.art)).map(|o| o.pfad.clone()).collect();
    let schatten = SchattenZiel { innen: ziel, dateien, lauf_id };
    let mut erg = direkt_ausfuehren(
        &teil,
        quelle,
        &schatten,
        lauf_id,
        opt,
        abbruch,
        &mut |o, st, m, u, h| journal(&plan.operationen[herkunft[o.nr]], st, m, u, h),
        fortschritt,
    );
    for f in &mut erg.fehler {
        f.nr = herkunft[f.nr];
    }
    erg
}

// --- Phasen 2 und 3: Aktionen der Reihe nach ------------------------------------

/// Eine Aktion des Umschaltens, Übernehmens oder Zurückrollens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Aktion {
    /// Nur wenn `von` (noch) eine Datei ist – sonst ist es schon geschehen.
    Umbenennen { von: RelPath, nach: RelPath, ersetzen: bool },
    /// Überschreiben umschalten: Ist die Schattendatei da, wird das Original (falls
    /// noch da) zur Sicherung und die Schattendatei zum Original. Ohne Schattendatei
    /// ist es schon geschehen.
    Tauschen { pfad: RelPath, schatten: RelPath, backup: RelPath },
    /// Rollback einer neuen Datei: Liegt die Schattendatei noch da, wurde nicht
    /// umgeschaltet – nur sie verschwindet; sonst die umgeschaltete Datei. Eine
    /// fremde Datei, die das Umschalten verhindert hat, bleibt so unberührt.
    NeuZurueck { pfad: RelPath, schatten: RelPath },
    DateiLoeschen(RelPath),
    OrdnerLoeschen(RelPath),
}

/// Ergebnis einer Phase aus Aktionen.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PhasenErgebnis {
    /// Operationen (Plan-Nummern), deren Aktionen alle gelungen sind.
    pub erledigt: Vec<usize>,
    /// Operationen mit Fehler: Plan-Nummer, Pfad, Meldung.
    pub fehler: Vec<(usize, RelPath, String)>,
    /// Nach einem Fehler gestoppt (nur Umschalten).
    pub gestoppt: bool,
}

impl PhasenErgebnis {
    pub fn vollstaendig(&self) -> bool {
        self.fehler.is_empty() && !self.gestoppt
    }
}

/// Rückmeldung je Operation: Operation, Status, ggf. Fehlermeldung.
pub type PhaseMelden<'a> = dyn FnMut(&Operation, JournalStatus, Option<&str>) -> Result<()> + 'a;

fn aktion(v: &mut dyn ZielVerbindung, a: &Aktion) -> Result<()> {
    match a {
        Aktion::Umbenennen { von, nach, ersetzen } => {
            if v.ist_datei(von)? {
                v.umbenennen(von, nach, *ersetzen)?;
            }
            Ok(())
        }
        Aktion::Tauschen { pfad, schatten, backup } => {
            if v.ist_datei(schatten)? {
                if v.ist_datei(pfad)? {
                    v.umbenennen(pfad, backup, false)?;
                }
                v.umbenennen(schatten, pfad, false)?;
            }
            Ok(())
        }
        Aktion::NeuZurueck { pfad, schatten } => {
            let weg = if v.ist_datei(schatten)? { schatten } else { pfad };
            v.datei_loeschen(weg)
        }
        Aktion::DateiLoeschen(p) => v.datei_loeschen(p),
        Aktion::OrdnerLoeschen(p) => v.ordner_loeschen(p),
    }
}

/// So oft wird die Verbindung für Umschalten, Übernehmen und Rollback versucht.
pub const VERBINDEN_VERSUCHE: usize = 3;

/// Verbindet; scheitert nur der Verbindungsaufbau (Kategorie `verbindung`), wird es
/// nach kurzer Pause erneut versucht – am Ziel ist dann noch nichts geschehen.
/// Gerade vor dem Umschalten soll ein kurzer Aussetzer nicht den Lauf anhalten.
fn verbinden(ziel: &dyn ZielEndpunkt) -> Result<Box<dyn ZielVerbindung + '_>> {
    let mut versuch = 1;
    loop {
        match ziel.verbinden() {
            Err(StellwerkError::Verbindung(_)) if versuch < VERBINDEN_VERSUCHE => {
                versuch += 1;
                std::thread::sleep(std::time::Duration::from_millis(300));
            }
            r => return r,
        }
    }
}

/// Führt die Aktionen je Operation aus. `stoppen`: nach dem ersten Fehler nichts
/// mehr beginnen. Ein Fehler beim Melden (Journal) beendet die Phase sofort.
fn phase(
    schritte: Vec<(&Operation, Vec<Aktion>)>,
    ziel: &dyn ZielEndpunkt,
    stoppen: bool,
    melden: &mut PhaseMelden,
) -> Result<PhasenErgebnis> {
    let mut erg = PhasenErgebnis::default();
    if schritte.is_empty() {
        return Ok(erg);
    }
    let mut v = verbinden(ziel)?;
    for (o, aktionen) in schritte {
        match aktionen.iter().try_for_each(|a| aktion(v.as_mut(), a)) {
            Ok(()) => {
                melden(o, JournalStatus::Erledigt, None)?;
                erg.erledigt.push(o.nr);
            }
            Err(e) => {
                let meldung = e.to_string();
                melden(o, JournalStatus::Fehler, Some(&meldung))?;
                erg.fehler.push((o.nr, o.pfad.clone(), meldung));
                if stoppen {
                    erg.gestoppt = true;
                    break;
                }
            }
        }
    }
    Ok(erg)
}

/// Phase 2: umschalten – nur Umbenennungen, stoppt beim ersten Fehler.
pub fn umschalten(plan: &Plan, ziel: &dyn ZielEndpunkt, lauf_id: &str, melden: &mut PhaseMelden) -> Result<PhasenErgebnis> {
    let schritte = plan
        .operationen
        .iter()
        .filter_map(|o| {
            let (schatten, bak) = (schatten_pfad(&o.pfad, lauf_id), backup_pfad(&o.pfad, lauf_id));
            let aktionen = match o.art {
                OpArt::Ueberschreiben => vec![Aktion::Tauschen { pfad: o.pfad.clone(), schatten, backup: bak }],
                OpArt::Kopieren => vec![Aktion::Umbenennen { von: schatten, nach: o.pfad.clone(), ersetzen: false }],
                OpArt::DateiLoeschen => vec![Aktion::Umbenennen { von: o.pfad.clone(), nach: bak, ersetzen: false }],
                _ => return None,
            };
            Some((o, aktionen))
        })
        .collect();
    phase(schritte, ziel, true, melden)
}

/// Phase 3: übernehmen – `.bak`-Dateien und zu löschende Ordner entfernen.
pub fn uebernehmen(plan: &Plan, ziel: &dyn ZielEndpunkt, lauf_id: &str, melden: &mut PhaseMelden) -> Result<PhasenErgebnis> {
    let schritte = plan
        .operationen
        .iter()
        .filter_map(|o| match o.art {
            OpArt::Ueberschreiben | OpArt::DateiLoeschen => Some((o, vec![Aktion::DateiLoeschen(backup_pfad(&o.pfad, lauf_id))])),
            OpArt::OrdnerLoeschen => Some((o, vec![Aktion::OrdnerLoeschen(o.pfad.clone())])),
            _ => None,
        })
        .collect();
    phase(schritte, ziel, false, melden)
}

/// Rollback nach (teilweisem) Umschalten: in umgekehrter Reihenfolge alles zurück –
/// Originale aus `.bak`, neue Dateien und Schattendateien weg, neue Ordner weg
/// (wenn leer). Läuft bei Fehlern weiter.
pub fn zurueckrollen(plan: &Plan, ziel: &dyn ZielEndpunkt, lauf_id: &str, melden: &mut PhaseMelden) -> Result<PhasenErgebnis> {
    let schritte = plan
        .operationen
        .iter()
        .rev()
        .filter_map(|o| {
            let (schatten, bak) = (schatten_pfad(&o.pfad, lauf_id), backup_pfad(&o.pfad, lauf_id));
            let aktionen = match o.art {
                // Ist die Sicherung da, kommt sie zurück; sonst wurde nicht umgeschaltet
                OpArt::Ueberschreiben => vec![
                    Aktion::Umbenennen { von: bak, nach: o.pfad.clone(), ersetzen: true },
                    Aktion::DateiLoeschen(schatten),
                ],
                OpArt::Kopieren => vec![Aktion::NeuZurueck { pfad: o.pfad.clone(), schatten }],
                OpArt::DateiLoeschen => vec![Aktion::Umbenennen { von: bak, nach: o.pfad.clone(), ersetzen: false }],
                OpArt::OrdnerAnlegen => vec![Aktion::OrdnerLoeschen(o.pfad.clone())],
                _ => return None,
            };
            Some((o, aktionen))
        })
        .collect();
    phase(schritte, ziel, false, melden)
}

/// Abbrechen vor dem Umschalten: Schattendateien und neue Ordner entfernen.
pub fn abbrechen(plan: &Plan, ziel: &dyn ZielEndpunkt, lauf_id: &str, melden: &mut PhaseMelden) -> Result<PhasenErgebnis> {
    let schritte = plan
        .operationen
        .iter()
        .rev()
        .filter_map(|o| match o.art {
            OpArt::Kopieren | OpArt::Ueberschreiben => Some((o, vec![Aktion::DateiLoeschen(schatten_pfad(&o.pfad, lauf_id))])),
            OpArt::OrdnerAnlegen => Some((o, vec![Aktion::OrdnerLoeschen(o.pfad.clone())])),
            _ => None,
        })
        .collect();
    phase(schritte, ziel, false, melden)
}
