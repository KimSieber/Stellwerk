//! Regeln für genau einen Pfad setzen oder entfernen – für das Kontextmenü
//! „Regel setzen“ der Vergleichsansicht. Die Strecke wird hier nur im Speicher
//! geändert; Prüfen und Speichern übernimmt der Aufrufer wie beim Editor.

use serde::{Deserialize, Serialize};

use crate::error::{Result, StellwerkError};
use crate::model::{RegelEbene, RegelTyp, Regel, RelPath, Strecke};

use super::Problem;

/// Wo die Regel gespeichert wird.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "ebene", rename_all = "snake_case")]
pub enum RegelOrt {
    /// Für alle Stufen der Strecke.
    Strecke,
    /// Nur für diese Stufe.
    Stufe { stufe: String },
}

/// gitignore-Muster, das genau diesen Pfad trifft: an der Wurzel verankert (`/…`),
/// Ordner mit abschließendem `/`, Sonderzeichen maskiert.
pub fn pfad_muster(pfad: &RelPath, ordner: bool) -> String {
    let mut m = String::from("/");
    for c in pfad.as_str().chars() {
        if matches!(c, '\\' | '*' | '?' | '[' | ']' | '{' | '}') {
            m.push('\\');
        }
        m.push(c);
    }
    // Abschließende Leerzeichen würde gitignore abschneiden
    let ohne = m.trim_end_matches(' ').len();
    if ohne < m.len() {
        let leer = m.len() - ohne;
        m.truncate(ohne);
        m.push_str(&"\\ ".repeat(leer));
    }
    if ordner {
        m.push('/');
    }
    m
}

fn ebene_mut<'a>(s: &'a mut Strecke, ort: &RegelOrt) -> Result<&'a mut RegelEbene> {
    match ort {
        RegelOrt::Strecke => Ok(&mut s.regeln),
        RegelOrt::Stufe { stufe } => {
            let id = s.id.clone();
            s.stufen
                .iter_mut()
                .find(|st| &st.id == stufe)
                .map(|st| &mut st.regeln)
                .ok_or_else(|| StellwerkError::NichtGefunden(format!("Stufe „{stufe}“ in Strecke „{id}“")))
        }
    }
}

/// Setzt auf der Ebene `ort` eine Regel für genau `pfad`. Eine vorhandene Regel mit
/// demselben Muster wird ersetzt (je Pfad und Ebene höchstens eine Regel).
pub fn regel_setzen(
    s: &mut Strecke,
    ort: &RegelOrt,
    pfad: &RelPath,
    ordner: bool,
    typ: RegelTyp,
    ersatz: Option<&RelPath>,
) -> Result<()> {
    let quelle = match (typ, ersatz, ordner) {
        (RegelTyp::Ersetzen, _, true) => return Err(ungueltig("„Ersetzen durch“ gilt nur für Dateien")),
        (RegelTyp::Ersetzen, None, _) => return Err(ungueltig("„Ersetzen durch“ braucht eine Ersatzdatei")),
        (RegelTyp::Ersetzen, Some(q), _) if q == pfad => {
            return Err(ungueltig("Die Ersatzdatei muss eine andere Datei sein"))
        }
        (RegelTyp::Ersetzen, Some(q), _) => Some(q.as_str().to_string()),
        (_, Some(_), _) => return Err(ungueltig("Eine Ersatzdatei gibt es nur bei „Ersetzen durch“")),
        (_, None, _) => None,
    };
    let muster = pfad_muster(pfad, ordner);
    let regel = Regel { muster: muster.clone(), typ, quelle, aktiv: true };
    let eigene = &mut ebene_mut(s, ort)?.eigene;
    match eigene.iter_mut().find(|r| r.muster == muster) {
        Some(vorhanden) => *vorhanden = regel,
        None => eigene.push(regel),
    }
    Ok(())
}

/// Entfernt auf der Ebene `ort` die Regel für genau `pfad`.
pub fn regel_entfernen(s: &mut Strecke, ort: &RegelOrt, pfad: &RelPath, ordner: bool) -> Result<()> {
    let muster = pfad_muster(pfad, ordner);
    let eigene = &mut ebene_mut(s, ort)?.eigene;
    let vorher = eigene.len();
    eigene.retain(|r| r.muster != muster);
    if eigene.len() == vorher {
        return Err(StellwerkError::NichtGefunden(format!("Regel „{muster}“")));
    }
    Ok(())
}

fn ungueltig(meldung: &str) -> StellwerkError {
    StellwerkError::Validierung(vec![Problem::fehler("regel", meldung)])
}
