//! Regel-Engine (KONZEPT Kap. 5, ARCHITEKTUR Kap. 6).
//!
//! - Drei Ebenen: Global → Strecke → Stufe. Jede Ebene kann Regeln der darüber
//!   liegenden per `standard_aus` abschalten; jede Regel hat einen Schalter (`aktiv`).
//! - Muster im gitignore-Format. Ausgewertet wird wie bei gitignore: die
//!   **letzte passende Regel gewinnt** – Reihenfolge Global, Strecke, Stufe,
//!   innerhalb einer Ebene in Listenreihenfolge. Damit schlägt die tiefere Ebene
//!   die höhere. Ein negiertes Muster (`!muster`) hebt vorherige Regeln auf.
//! - Jeder Treffer nennt seine Herkunft (Ebene, Index, Muster) für die Oberfläche.

use std::collections::HashSet;

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use ignore::Match;
use serde::Serialize;
use unicode_normalization::UnicodeNormalization;

use crate::config::{GlobalConfig, Problem};
use crate::error::{Result, StellwerkError};
use crate::model::{Regel, RegelTyp, RelPath, Strecke};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Ebene {
    Global,
    Strecke,
    Stufe,
}

/// Wodurch eine Regel abgeschaltet ist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Abschaltung {
    /// Schalter der Regel selbst (`aktiv = false`).
    Schalter,
    /// `standard_aus` der Strecke.
    Strecke,
    /// `standard_aus` der Stufe.
    Stufe,
}

/// Herkunft einer Regel.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct RegelHerkunft {
    pub ebene: Ebene,
    /// Index in der Liste ihrer Ebene.
    pub index: usize,
    pub muster: String,
}

/// Die für einen Pfad greifende Regel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegelTreffer {
    pub typ: RegelTyp,
    pub herkunft: RegelHerkunft,
    /// Nur bei „Ersetzen durch“: die Quelldatei, die stattdessen übertragen wird.
    pub ersatz: Option<RelPath>,
}

/// Eine Zeile der Regelübersicht – auch abgeschaltete Regeln.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegelEintrag {
    pub herkunft: RegelHerkunft,
    /// Ort in der Konfiguration, z. B. `stufen[1].regeln.eigene[2]`.
    pub ort: String,
    pub typ: RegelTyp,
    pub quelle: Option<String>,
    pub abgeschaltet: Option<Abschaltung>,
}

impl RegelEintrag {
    pub fn wirksam(&self) -> bool {
        self.abgeschaltet.is_none()
    }
}

#[derive(Debug)]
struct Kompiliert {
    matcher: Gitignore,
    treffer: RegelTreffer,
}

/// Die wirksamen Regeln einer Stufe, fertig zum Auswerten.
#[derive(Debug)]
pub struct RegelWerk {
    regeln: Vec<Kompiliert>,
}

impl RegelWerk {
    /// Alle Regeln, die für eine Stufe in Betracht kommen, in Auswertungsreihenfolge,
    /// jeweils mit Herkunft und ggf. Grund der Abschaltung.
    pub fn uebersicht(global: &GlobalConfig, strecke: &Strecke, stufe: usize) -> Result<Vec<RegelEintrag>> {
        let st = strecke.stufen.get(stufe).ok_or_else(|| {
            StellwerkError::NichtGefunden(format!("Stufe Nr. {} in Strecke '{}'", stufe + 1, strecke.id))
        })?;
        let aus_strecke = menge(&strecke.regeln.standard_aus);
        let aus_stufe = menge(&st.regeln.standard_aus);

        let mut liste = Vec::new();
        let mut ebene = |regeln: &[Regel], e: Ebene, ort: &str, abschalter: &[(&HashSet<&str>, Abschaltung)]| {
            for (index, r) in regeln.iter().enumerate() {
                let abgeschaltet = if r.aktiv {
                    abschalter.iter().find(|(m, _)| m.contains(r.muster.as_str())).map(|(_, a)| *a)
                } else {
                    Some(Abschaltung::Schalter)
                };
                liste.push(RegelEintrag {
                    herkunft: RegelHerkunft { ebene: e, index, muster: r.muster.clone() },
                    ort: format!("{ort}[{index}]"),
                    typ: r.typ,
                    quelle: r.quelle.clone(),
                    abgeschaltet,
                });
            }
        };
        ebene(
            &global.regeln.standard,
            Ebene::Global,
            "global.regeln.standard",
            &[(&aus_strecke, Abschaltung::Strecke), (&aus_stufe, Abschaltung::Stufe)],
        );
        ebene(&strecke.regeln.eigene, Ebene::Strecke, "regeln.eigene", &[(&aus_stufe, Abschaltung::Stufe)]);
        ebene(&st.regeln.eigene, Ebene::Stufe, &format!("stufen[{stufe}].regeln.eigene"), &[]);
        Ok(liste)
    }

    /// Baut das Regelwerk einer Stufe. Ungültige Muster oder Ersatzquellen werden
    /// gesammelt als [`StellwerkError::Validierung`] gemeldet.
    pub fn fuer_stufe(global: &GlobalConfig, strecke: &Strecke, stufe: usize) -> Result<Self> {
        let mut regeln = Vec::new();
        let mut probleme = Vec::new();
        for e in Self::uebersicht(global, strecke, stufe)?.into_iter().filter(RegelEintrag::wirksam) {
            let matcher =
                kompiliere(&e.herkunft.muster).map_err(|m| Problem::fehler(format!("{}.muster", e.ort), m));
            let ersatz =
                ersatz(e.typ, e.quelle.as_deref()).map_err(|m| Problem::fehler(format!("{}.quelle", e.ort), m));
            match (matcher, ersatz) {
                (Ok(matcher), Ok(ersatz)) => regeln.push(Kompiliert {
                    matcher,
                    treffer: RegelTreffer { typ: e.typ, herkunft: e.herkunft, ersatz },
                }),
                (m, er) => probleme.extend(m.err().into_iter().chain(er.err())),
            }
        }
        if probleme.is_empty() {
            Ok(Self { regeln })
        } else {
            Err(StellwerkError::Validierung(probleme))
        }
    }

    /// Die für `pfad` greifende Regel. Ordnermuster (`logs/`) gelten auch für
    /// alles darin.
    pub fn treffer(&self, pfad: &RelPath, ist_ordner: bool) -> Option<&RegelTreffer> {
        for r in self.regeln.iter().rev() {
            match r.matcher.matched_path_or_any_parents(pfad.as_str(), ist_ordner) {
                Match::None => continue,
                Match::Ignore(_) => return Some(&r.treffer),
                Match::Whitelist(_) => return None,
            }
        }
        None
    }
}

fn menge(liste: &[String]) -> HashSet<&str> {
    liste.iter().map(String::as_str).collect()
}

/// Ein einzelnes Muster als gitignore-Matcher. Muster werden wie Pfade nach NFC normalisiert.
fn kompiliere(muster: &str) -> std::result::Result<Gitignore, String> {
    let nfc: String = muster.nfc().collect();
    let mut b = GitignoreBuilder::new("");
    b.add_line(None, &nfc).map_err(fehlertext)?;
    b.build().map_err(fehlertext)
}

fn fehlertext(e: ignore::Error) -> String {
    format!("ungültiges Muster: {e}")
}

/// Fehlermeldung, falls `muster` kein gültiges gitignore-Muster ist.
pub fn muster_fehler(muster: &str) -> Option<String> {
    kompiliere(muster).err()
}

/// Prüft das Feld `quelle` einer Regel und liefert ggf. den Ersatzpfad.
pub(crate) fn ersatz(typ: RegelTyp, quelle: Option<&str>) -> std::result::Result<Option<RelPath>, String> {
    match (typ, quelle) {
        (RegelTyp::Ersetzen, None) => Err("„Ersetzen durch“ braucht eine Quelldatei".into()),
        (RegelTyp::Ersetzen, Some(q)) => match RelPath::new(q) {
            Ok(p) if p.ist_wurzel() => Err("Quelldatei fehlt".into()),
            Ok(p) => Ok(Some(p)),
            Err(e) => Err(e.to_string()),
        },
        (_, Some(_)) => Err("„quelle“ ist nur bei Typ „ersetzen“ erlaubt".into()),
        (_, None) => Ok(None),
    }
}
