//! Plan: aus den angehakten Zeilen eines Vergleichs werden geordnete Operationen
//! mit Abhängigkeiten (ARCHITEKTUR Kap. 7 Schritt 3). Der Plan verändert nichts –
//! erst die Ausführung schreibt, und zwar nur auf das Ziel (E3).
//!
//! Reihenfolge:
//! 1. **Vorgezogene Löschungen** – nur bei Groß/Klein-Paaren: `Bild.jpg` neu und
//!    `bild.jpg` löschen ist auf macOS dieselbe Datei, also erst löschen, dann anlegen.
//! 2. **Ordner anlegen** (auch leere), Eltern vor Kindern.
//! 3. **Dateien** kopieren (neu) bzw. überschreiben (geändert). Bei „Ersetzen durch“
//!    kommt der Inhalt aus der Ersatzdatei der Quelle.
//! 4. **Rechte** setzen – nur wenn die Stufe es verlangt, nur für angelegte und
//!    überschriebene Einträge, Ordner nach allem, was darin geschrieben wird.
//! 5. **Löschungen**, tiefste Pfade zuerst.
//!
//! Jede Operation nennt die Operationen, die vorher fertig sein müssen (`nach`).
//! Sie stehen immer weiter vorn im Plan – der Worker-Pool nimmt nur „freie“
//! Operationen (ARCHITEKTUR Kap. 8.1).

use std::collections::{HashMap, HashSet};

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use ignore::Match;
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

use crate::diff::{DiffEintrag, DiffStatus};
use crate::error::{Result, StellwerkError};
use crate::model::{Dateimodus, EintragArt, Rechte, RelPath};

/// Art einer Operation (Journal-Spalte `op`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpArt {
    OrdnerAnlegen,
    /// Datei neu am Ziel.
    Kopieren,
    /// Datei am Ziel ersetzen.
    Ueberschreiben,
    RechteSetzen,
    DateiLoeschen,
    /// Leeren Ordner löschen – sein Inhalt wird vorher gelöscht.
    OrdnerLoeschen,
}

impl OpArt {
    pub fn ist_loeschen(self) -> bool {
        matches!(self, OpArt::DateiLoeschen | OpArt::OrdnerLoeschen)
    }
}

/// Eine geplante Operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Operation {
    /// Laufende Nummer = Index im Plan.
    pub nr: usize,
    pub art: OpArt,
    /// Pfad am Ziel.
    pub pfad: RelPath,
    /// Nur Kopieren/Überschreiben: Pfad in der Quelle (bei „Ersetzen durch“ die Ersatzdatei).
    pub quelle: Option<RelPath>,
    /// Bytes, die übertragen werden (nur Kopieren/Überschreiben).
    pub groesse: u64,
    /// Nur Rechte setzen.
    pub modus: Option<Dateimodus>,
    /// Nummern der Operationen, die vorher fertig sein müssen (alle kleiner als `nr`).
    pub nach: Vec<usize>,
}

/// Zahlen für den Bestätigungsdialog.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanZusammenfassung {
    pub ordner_anlegen: usize,
    pub kopieren: usize,
    pub ueberschreiben: usize,
    pub rechte: usize,
    pub dateien_loeschen: usize,
    pub ordner_loeschen: usize,
    /// Summe der zu übertragenden Bytes.
    pub bytes: u64,
}

/// Hinweis zum Plan – verhindert nichts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "art", rename_all = "snake_case")]
pub enum PlanWarnung {
    /// Ein Modus macht Einträge für alle schreibbar (z. B. 777).
    Weltschreibbar { modus: Dateimodus, anzahl: usize },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Plan {
    pub operationen: Vec<Operation>,
    pub zusammenfassung: PlanZusammenfassung,
    /// Alle Pfade, die gelöscht werden – für die Sammelbestätigung (E5), sortiert.
    pub loeschliste: Vec<RelPath>,
    pub warnungen: Vec<PlanWarnung>,
}

impl Plan {
    pub fn ist_leer(&self) -> bool {
        self.operationen.is_empty()
    }

    pub fn hat_loeschungen(&self) -> bool {
        !self.loeschliste.is_empty()
    }
}

/// Erstellt den Plan aus den Einträgen eines Vergleichs. Berücksichtigt werden nur
/// angehakte Zeilen mit Status neu, geändert oder löschen.
///
/// Fehler (es wird dann nichts geplant):
/// - ein angehakter Eintrag liegt in einem neuen Ordner, der nicht angehakt ist,
/// - ein angehakter Eintrag ist kein Ordner und keine Datei (Symlink).
pub fn plan_erstellen(eintraege: &[DiffEintrag], rechte: &Rechte) -> Result<Plan> {
    let gewaehlt: Vec<&DiffEintrag> = eintraege
        .iter()
        .filter(|e| e.ausgewaehlt && matches!(e.status, DiffStatus::Neu | DiffStatus::Geaendert | DiffStatus::Loeschen))
        .collect();
    let gewaehlt_pfade: HashSet<&RelPath> = gewaehlt.iter().map(|e| &e.pfad).collect();
    let neue_ordner_ungewaehlt: HashSet<&RelPath> = eintraege
        .iter()
        .filter(|e| e.status == DiffStatus::Neu && !gewaehlt_pfade.contains(&e.pfad))
        .map(|e| &e.pfad)
        .collect();

    let mut anlegen: Vec<(&RelPath, Option<&RelPath>, EintragArt, u64, OpArt)> = Vec::new();
    let mut loeschen: Vec<(&RelPath, bool)> = Vec::new();
    for e in &gewaehlt {
        if e.status == DiffStatus::Loeschen {
            let z = e.ziel.as_ref().expect("„löschen“ hat immer ein Ziel");
            loeschen.push((&e.pfad, art_pruefen(&e.pfad, z.art)? == EintragArt::Ordner));
            continue;
        }
        if let Some(v) = vorfahren(&e.pfad).find(|v| neue_ordner_ungewaehlt.contains(v)) {
            return Err(StellwerkError::NichtUnterstuetzt(format!(
                "„{}“ liegt im neuen Ordner „{v}“, der nicht ausgewählt ist",
                e.pfad
            )));
        }
        let q = e.quelle.as_ref().expect("neu/geändert hat immer eine Quelle");
        let art = art_pruefen(&e.pfad, q.art)?;
        let op = match (art, e.status) {
            (EintragArt::Ordner, _) => OpArt::OrdnerAnlegen,
            (_, DiffStatus::Neu) => OpArt::Kopieren,
            _ => OpArt::Ueberschreiben,
        };
        let quelle = (art == EintragArt::Datei).then_some(&q.pfad);
        anlegen.push((&e.pfad, quelle, art, if art == EintragArt::Datei { q.groesse } else { 0 }, op));
    }

    // Groß/Klein: Löschungen, deren Pfad (gefaltet) angelegt wird, kommen zuerst –
    // mit allem, was darunter gelöscht wird.
    let anlegen_gefaltet: HashSet<String> = anlegen.iter().map(|a| a.0.faltung()).collect();
    let vorziehen = |p: &RelPath| {
        std::iter::once(p.clone()).chain(vorfahren(p)).any(|x| anlegen_gefaltet.contains(&x.faltung()))
    };
    let tiefste_zuerst = |a: &(&RelPath, bool), b: &(&RelPath, bool)| {
        b.0.tiefe().cmp(&a.0.tiefe()).then_with(|| a.0.cmp(b.0))
    };
    let (mut frueh, mut spaet): (Vec<_>, Vec<_>) = loeschen.iter().copied().partition(|(p, _)| vorziehen(p));
    frueh.sort_by(tiefste_zuerst);
    spaet.sort_by(tiefste_zuerst);
    // Ordner vor Dateien, jeweils flach vor tief, dann nach Pfad
    anlegen.sort_by(|a, b| {
        (a.2 != EintragArt::Ordner).cmp(&(b.2 != EintragArt::Ordner)).then(a.0.tiefe().cmp(&b.0.tiefe())).then(a.0.cmp(b.0))
    });

    let rechte_regeln = RechteAuswertung::neu(rechte)?;
    let mut b = Bauer::default();
    let mut loesch_nr: HashMap<&RelPath, usize> = HashMap::new();
    let mut frueh_gefaltet: HashMap<String, Vec<usize>> = HashMap::new();
    for (p, ordner) in &frueh {
        let nr = b.loeschen(p, *ordner, &loesch_nr);
        loesch_nr.insert(p, nr);
        frueh_gefaltet.entry(p.faltung()).or_default().push(nr);
    }
    let mut angelegt: HashMap<&RelPath, usize> = HashMap::new();
    for (p, quelle, _, groesse, art) in &anlegen {
        let mut nach: Vec<usize> = p.parent().and_then(|v| angelegt.get(&v).copied()).into_iter().collect();
        nach.extend(frueh_gefaltet.get(&p.faltung()).into_iter().flatten());
        nach.sort_unstable();
        let nr = b.push(*art, p, quelle.cloned(), *groesse, None, nach);
        angelegt.insert(p, nr);
    }
    // Rechte: Dateien zuerst, dann Ordner von tief nach flach – so hängt ein Ordner
    // nur von bereits geplanten Operationen ab.
    let mut fuer_rechte: Vec<_> = anlegen.iter().filter_map(|a| rechte_regeln.modus(a.0, a.2).map(|m| (a, m))).collect();
    fuer_rechte.sort_by(|(a, _), (b, _)| {
        (a.2 == EintragArt::Ordner).cmp(&(b.2 == EintragArt::Ordner)).then(b.0.tiefe().cmp(&a.0.tiefe())).then(a.0.cmp(b.0))
    });
    let mut im_teilbaum: HashMap<RelPath, Vec<usize>> = HashMap::new();
    for (p, nr) in &angelegt {
        for v in vorfahren(p) {
            im_teilbaum.entry(v).or_default().push(*nr);
        }
    }
    let mut warn: HashMap<Dateimodus, usize> = HashMap::new();
    for ((p, _, art, _, _), modus) in fuer_rechte {
        let mut nach = vec![angelegt[p]];
        if *art == EintragArt::Ordner {
            nach.extend(im_teilbaum.get(*p).into_iter().flatten());
        }
        nach.sort_unstable();
        let nr = b.push(OpArt::RechteSetzen, p, None, 0, Some(modus), nach);
        for v in vorfahren(p) {
            im_teilbaum.entry(v).or_default().push(nr);
        }
        if modus.ist_weltschreibbar() {
            *warn.entry(modus).or_default() += 1;
        }
    }
    for (p, ordner) in &spaet {
        let nr = b.loeschen(p, *ordner, &loesch_nr);
        loesch_nr.insert(p, nr);
    }

    let mut loeschliste: Vec<RelPath> = loeschen.iter().map(|(p, _)| (*p).clone()).collect();
    loeschliste.sort();
    let mut warnungen: Vec<PlanWarnung> =
        warn.into_iter().map(|(modus, anzahl)| PlanWarnung::Weltschreibbar { modus, anzahl }).collect();
    warnungen.sort_by_key(|PlanWarnung::Weltschreibbar { modus, .. }| modus.bits());
    let zusammenfassung = zusammenfassen(&b.ops);
    Ok(Plan { operationen: b.ops, zusammenfassung, loeschliste, warnungen })
}

fn art_pruefen(pfad: &RelPath, art: EintragArt) -> Result<EintragArt> {
    match art {
        EintragArt::Symlink => {
            Err(StellwerkError::NichtUnterstuetzt(format!("„{pfad}“ ist ein symbolischer Link und wird nicht übertragen")))
        }
        a => Ok(a),
    }
}

#[derive(Default)]
struct Bauer {
    ops: Vec<Operation>,
}

impl Bauer {
    fn push(
        &mut self,
        art: OpArt,
        pfad: &RelPath,
        quelle: Option<RelPath>,
        groesse: u64,
        modus: Option<Dateimodus>,
        nach: Vec<usize>,
    ) -> usize {
        let nr = self.ops.len();
        self.ops.push(Operation { nr, art, pfad: pfad.clone(), quelle, groesse, modus, nach });
        nr
    }

    /// Löschung; ein Ordner wartet auf die Löschung seines direkten Inhalts
    /// (tiefste Pfade stehen vorn, der Inhalt ist also schon geplant).
    fn loeschen(&mut self, p: &RelPath, ordner: bool, bisher: &HashMap<&RelPath, usize>) -> usize {
        let (art, mut nach) = if ordner {
            let kinder: Vec<usize> =
                bisher.iter().filter(|(k, _)| k.parent().as_ref() == Some(p)).map(|(_, &nr)| nr).collect();
            (OpArt::OrdnerLoeschen, kinder)
        } else {
            (OpArt::DateiLoeschen, vec![])
        };
        nach.sort_unstable();
        self.push(art, p, None, 0, None, nach)
    }
}

fn zusammenfassen(ops: &[Operation]) -> PlanZusammenfassung {
    let mut z = PlanZusammenfassung::default();
    for o in ops {
        *match o.art {
            OpArt::OrdnerAnlegen => &mut z.ordner_anlegen,
            OpArt::Kopieren => &mut z.kopieren,
            OpArt::Ueberschreiben => &mut z.ueberschreiben,
            OpArt::RechteSetzen => &mut z.rechte,
            OpArt::DateiLoeschen => &mut z.dateien_loeschen,
            OpArt::OrdnerLoeschen => &mut z.ordner_loeschen,
        } += 1;
        z.bytes += o.groesse;
    }
    z
}

/// Alle Vorfahren eines Pfads ohne die Wurzel, vom nächsten aufwärts.
fn vorfahren(p: &RelPath) -> impl Iterator<Item = RelPath> {
    std::iter::successors(p.parent(), RelPath::parent).filter(|v| !v.ist_wurzel())
}

/// Dateirechte der Stufe (KONZEPT Kap. 8): nicht setzen · feste Werte · nach Muster.
/// Bei Mustern gewinnt die letzte passende Regel (wie bei gitignore); passt keine,
/// gilt der Vorgabewert für Ordner bzw. Dateien – fehlt er, bleibt der Eintrag unverändert.
struct RechteAuswertung {
    ordner: Option<Dateimodus>,
    dateien: Option<Dateimodus>,
    regeln: Vec<(Gitignore, Dateimodus)>,
}

impl RechteAuswertung {
    fn neu(r: &Rechte) -> Result<Self> {
        Ok(match r {
            Rechte::NichtSetzen => Self { ordner: None, dateien: None, regeln: vec![] },
            Rechte::Fest { ordner, dateien } => Self { ordner: *ordner, dateien: *dateien, regeln: vec![] },
            Rechte::Muster { ordner, dateien, regeln } => Self {
                ordner: *ordner,
                dateien: *dateien,
                regeln: regeln
                    .iter()
                    .enumerate()
                    .map(|(i, r)| {
                        let mut b = GitignoreBuilder::new("");
                        let fehler = |e: ignore::Error| {
                            StellwerkError::Validierung(vec![crate::config::Problem::fehler(
                                format!("rechte.regeln[{i}].muster"),
                                format!("ungültiges Muster: {e}"),
                            )])
                        };
                        b.add_line(None, &r.muster.nfc().collect::<String>()).map_err(fehler)?;
                        Ok((b.build().map_err(fehler)?, r.modus))
                    })
                    .collect::<Result<_>>()?,
            },
        })
    }

    fn modus(&self, pfad: &RelPath, art: EintragArt) -> Option<Dateimodus> {
        let ordner = art == EintragArt::Ordner;
        self.regeln
            .iter()
            .rev()
            .find(|(m, _)| matches!(m.matched_path_or_any_parents(pfad.as_str(), ordner), Match::Ignore(_)))
            .map(|(_, modus)| *modus)
            .or(if ordner { self.ordner } else { self.dateien })
    }
}
