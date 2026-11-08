//! Ein geöffneter Vergleich: hält das Ergebnis, liefert sichtbare Zeilen
//! seitenweise und verwaltet die Häkchen (ARCHITEKTUR Kap. 9: `diff_seite`,
//! `auswahl_setzen`). Die Oberfläche rechnet nichts (A1) – auch Filter und
//! Auf-/Zuklappen des Baums werden hier ausgewertet.
//!
//! Auswahl (vorsichtige Auslegung):
//! - Wählbar ist nur, was der Vergleich vorausgewählt hat (neu, geändert, löschen
//!   ohne Konfliktkopie, ohne „nur online – überspringen“, nicht unter einem Konflikt).
//!   Alles andere bleibt ohne Häkchen.
//! - Ein Häkchen an einem Ordner wirkt auf seinen gesamten Inhalt.
//! - Wird etwas Neues angehakt, werden neue Ordner darüber mit angehakt
//!   (ohne sie kann es nicht angelegt werden).
//! - Ein zu löschender Ordner verliert sein Häkchen, sobald darin etwas bleibt.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::sortierung::pfade_vergleichen;
use crate::config::pfad_muster;
use crate::diff::{zusammenfassen, DiffEintrag, DiffStatus, Vergleich, Zusammenfassung};
use crate::error::{Result, StellwerkError};
use crate::model::{EintragArt, RelPath};
use crate::rules::Ebene;

/// Filter der Vergleichsansicht.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Filter {
    /// neu, geändert, löschen, Konflikt
    #[default]
    Aenderungen,
    Alle,
    /// Zeilen mit Hinweisen oder Konflikt
    Warnungen,
}

impl Filter {
    fn passt(self, e: &DiffEintrag) -> bool {
        match self {
            Filter::Alle => true,
            Filter::Aenderungen => matches!(
                e.status,
                DiffStatus::Neu | DiffStatus::Geaendert | DiffStatus::Loeschen | DiffStatus::Konflikt
            ),
            Filter::Warnungen => e.status == DiffStatus::Konflikt || !e.warnungen.is_empty(),
        }
    }
}

/// Eine Zeile der Ansicht: Diff-Eintrag plus Baum-Angaben.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Zeile {
    #[serde(flatten)]
    pub eintrag: DiffEintrag,
    /// Letzter Pfadteil.
    pub name: String,
    /// Einrückung (0 = oberste Ebene).
    pub tiefe: usize,
    /// Eintrag ist (auf mindestens einer Seite) ein Ordner.
    pub ordner: bool,
    /// Ordner hat unter dem aktuellen Filter sichtbaren Inhalt.
    pub kinder: bool,
    pub zugeklappt: bool,
    /// Häkchen kann gesetzt werden.
    pub auswaehlbar: bool,
    /// Auf der Zeile greift eine Regel der Strecke oder Stufe mit genau diesem Pfad –
    /// sie lässt sich über das Kontextmenü wieder entfernen.
    pub regel_entfernbar: bool,
}

/// Ausschnitt der sichtbaren Zeilen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Seite {
    pub zeilen: Vec<Zeile>,
    /// Anzahl aller sichtbaren Zeilen (für die Länge der Liste).
    pub sichtbar: usize,
}

pub struct VergleichsSitzung {
    /// In Anzeige-Reihenfolge (Baum, Ordner zuerst, deutsch sortiert).
    eintraege: Vec<DiffEintrag>,
    ordner: Vec<bool>,
    /// Nächster vorhandener Vorfahr.
    eltern: Vec<Option<usize>>,
    tiefe: Vec<usize>,
    /// Ende (exklusiv) des Teilbaums – der Inhalt folgt in der Reihenfolge direkt.
    teilbaum_ende: Vec<usize>,
    auswaehlbar: Vec<bool>,
    index: HashMap<RelPath, usize>,
    zusammenfassung: Zusammenfassung,
}

impl VergleichsSitzung {
    pub fn neu(v: Vergleich) -> Self {
        let ist_ordner = |e: &DiffEintrag| [&e.quelle, &e.ziel].into_iter().flatten().any(|x| x.ist_ordner());
        let mut paare: Vec<(DiffEintrag, bool)> = v.eintraege.into_iter().map(|e| { let o = ist_ordner(&e); (e, o) }).collect();
        paare.sort_by(|(a, ao), (b, bo)| pfade_vergleichen(&a.pfad, *ao, &b.pfad, *bo));
        let (eintraege, ordner): (Vec<_>, Vec<_>) = paare.into_iter().unzip();

        let index: HashMap<RelPath, usize> = eintraege.iter().enumerate().map(|(i, e)| (e.pfad.clone(), i)).collect();
        let eltern: Vec<Option<usize>> = eintraege
            .iter()
            .map(|e| std::iter::successors(e.pfad.parent(), RelPath::parent).find_map(|v| index.get(&v).copied()))
            .collect();
        // Eltern stehen immer vor ihrem Inhalt → ein Durchlauf genügt
        let mut tiefe = vec![0; eintraege.len()];
        for i in 0..eintraege.len() {
            tiefe[i] = eltern[i].map_or(0, |p| tiefe[p] + 1);
        }
        let mut teilbaum_ende: Vec<usize> = (1..=eintraege.len()).collect();
        for i in (0..eintraege.len()).rev() {
            if let Some(p) = eltern[i] {
                teilbaum_ende[p] = teilbaum_ende[p].max(teilbaum_ende[i]);
            }
        }
        let auswaehlbar = eintraege.iter().map(|e| e.ausgewaehlt).collect();
        let zusammenfassung = zusammenfassen(&eintraege);
        Self { eintraege, ordner, eltern, tiefe, teilbaum_ende, auswaehlbar, index, zusammenfassung }
    }

    pub fn zusammenfassung(&self) -> &Zusammenfassung {
        &self.zusammenfassung
    }

    /// Anzahl aller Zeilen ohne Filter.
    pub fn anzahl(&self) -> usize {
        self.eintraege.len()
    }

    /// Alle Einträge mit aktuellem Häkchen-Stand (Grundlage für den Plan).
    pub fn eintraege(&self) -> &[DiffEintrag] {
        &self.eintraege
    }

    /// Dateien, deren Inhalt am Ziel unbekannt ist („Inhalt nicht geprüft“) – Pfad am
    /// Ziel und Größe, für „Inhalte prüfen“.
    pub fn ungeprueft(&self) -> Vec<(RelPath, u64)> {
        self.eintraege
            .iter()
            .filter(|e| e.warnungen.contains(&crate::model::Warnung::InhaltNichtGeprueft))
            .map(|e| (e.pfad.clone(), e.ziel.as_ref().map_or(0, |z| z.groesse)))
            .collect()
    }

    /// Sichtbare Zeilen ab `offset`, höchstens `limit`. Ein Eintrag ist sichtbar, wenn er
    /// zum Filter passt oder etwas Passendes enthält und kein Vorfahr zugeklappt ist.
    pub fn seite(&self, offset: usize, limit: usize, filter: Filter, zugeklappt: &HashSet<RelPath>) -> Seite {
        let n = self.eintraege.len();
        let mut passt = vec![false; n];
        for i in (0..n).filter(|&i| filter.passt(&self.eintraege[i])) {
            let mut k = Some(i);
            while let Some(j) = k.filter(|&j| !passt[j]) {
                passt[j] = true;
                k = self.eltern[j];
            }
        }
        let mut kinder = vec![false; n];
        let mut versteckt = vec![false; n];
        for i in 0..n {
            if let Some(p) = self.eltern[i] {
                kinder[p] |= passt[i];
                versteckt[i] = versteckt[p] || zugeklappt.contains(&self.eintraege[p].pfad);
            }
        }
        let sichtbar: Vec<usize> = (0..n).filter(|&i| passt[i] && !versteckt[i]).collect();
        let zeilen = sichtbar
            .iter()
            .skip(offset)
            .take(limit)
            .map(|&i| {
                let e = &self.eintraege[i];
                Zeile {
                    eintrag: e.clone(),
                    name: e.pfad.name().to_string(),
                    tiefe: self.tiefe[i],
                    ordner: self.ordner[i],
                    kinder: kinder[i],
                    zugeklappt: zugeklappt.contains(&e.pfad),
                    auswaehlbar: self.auswaehlbar[i],
                    regel_entfernbar: e.regel.as_ref().is_some_and(|r| {
                        r.herkunft.ebene != Ebene::Global && r.herkunft.muster == pfad_muster(&e.pfad, self.ordner[i])
                    }),
                }
            })
            .collect();
        Seite { zeilen, sichtbar: sichtbar.len() }
    }

    /// Dateien der Quelle im selben Ordner wie `pfad` (ohne `pfad` selbst) –
    /// Vorschläge für „Ersetzen durch …“.
    pub fn quelldateien_neben(&self, pfad: &RelPath) -> Vec<RelPath> {
        let ordner = pfad.parent();
        self.eintraege
            .iter()
            .filter_map(|e| e.quelle.as_ref())
            .filter(|q| q.art == EintragArt::Datei && q.pfad.parent() == ordner && &q.pfad != pfad)
            .map(|q| q.pfad.clone())
            .collect()
    }

    /// Setzt oder entfernt Häkchen (siehe Modulbeschreibung). Unbekannte Pfade sind
    /// ein Fehler; dann wird nichts geändert.
    pub fn auswahl_setzen(&mut self, pfade: &[RelPath], an: bool) -> Result<&Zusammenfassung> {
        let start: Vec<usize> = pfade
            .iter()
            .map(|p| self.index.get(p).copied().ok_or_else(|| StellwerkError::NichtGefunden(format!("Zeile „{p}“"))))
            .collect::<Result<_>>()?;
        for i in start {
            for j in (i..self.teilbaum_ende[i]).filter(|&j| self.auswaehlbar[j]) {
                self.eintraege[j].ausgewaehlt = an;
            }
            let mut k = self.eltern[i];
            while let Some(v) = k.filter(|_| an) {
                if self.auswaehlbar[v] && self.eintraege[v].status == DiffStatus::Neu {
                    self.eintraege[v].ausgewaehlt = true;
                }
                k = self.eltern[v];
            }
        }
        self.bleibendes_abwaehlen();
        self.zusammenfassung = zusammenfassen(&self.eintraege);
        Ok(&self.zusammenfassung)
    }

    /// Ein zu löschender Ordner darf nur weg, wenn sein gesamter Inhalt mit gelöscht wird.
    fn bleibendes_abwaehlen(&mut self) {
        let bleibt =
            |e: &DiffEintrag| e.ziel.is_some() && !(e.status == DiffStatus::Loeschen && e.ausgewaehlt);
        let mut blockiert = vec![false; self.eintraege.len()];
        for i in (0..self.eintraege.len()).filter(|&i| bleibt(&self.eintraege[i])) {
            let mut k = self.eltern[i];
            while let Some(v) = k.filter(|&v| !blockiert[v]) {
                blockiert[v] = true;
                k = self.eltern[v];
            }
        }
        for (e, b) in self.eintraege.iter_mut().zip(blockiert) {
            if b && e.status == DiffStatus::Loeschen {
                e.ausgewaehlt = false;
            }
        }
    }
}
