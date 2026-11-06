//! Befehle für das Frontend (ARCHITEKTUR Kap. 9). Hier wird nur übersetzt, nicht gerechnet.

use serde::Serialize;
use tauri::State;

use stellwerk_core::config::{validiere_strecke, Problem};
use stellwerk_core::model::{Endpunkt, Quelle, Strecke};
use stellwerk_core::StellwerkError;

use crate::state::AppState;

/// Fehler, wie ihn das Frontend erhält.
#[derive(Debug, Serialize)]
pub struct BefehlsFehler {
    pub kategorie: &'static str,
    pub meldung: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub probleme: Vec<Problem>,
}

impl From<StellwerkError> for BefehlsFehler {
    fn from(e: StellwerkError) -> Self {
        let meldung = e.to_string();
        match e {
            StellwerkError::Validierung(probleme) => {
                Self { kategorie: "validierung", meldung, probleme }
            }
            StellwerkError::Konfiguration { .. } => {
                Self { kategorie: "konfiguration", meldung, probleme: vec![] }
            }
            StellwerkError::NichtGefunden(_) => {
                Self { kategorie: "nicht_gefunden", meldung, probleme: vec![] }
            }
            StellwerkError::Io { .. } => Self { kategorie: "io", meldung, probleme: vec![] },
        }
    }
}

type Antwort<T> = Result<T, BefehlsFehler>;

#[derive(Debug, Serialize)]
pub struct StufeKurz {
    pub id: String,
    pub name: String,
    pub quelle: String,
    pub ziel: String,
}

/// Eintrag der Startseite. Defekte Dateien erscheinen mit `fehler`.
#[derive(Debug, Serialize)]
pub struct StreckeUebersicht {
    pub id: String,
    pub name: Option<String>,
    pub stufen: Vec<StufeKurz>,
    pub probleme: Vec<Problem>,
    pub fehler: Option<String>,
}

fn quelle_kurz(q: &Quelle) -> String {
    match q {
        Quelle::Lokal { pfad } => pfad.display().to_string(),
        Quelle::Ftps(f) => format!("ftps://{}{}", f.host, f.pfad),
        Quelle::Stufe { stufe } => format!("Ziel von „{stufe}“"),
    }
}

fn ziel_kurz(z: &Endpunkt) -> String {
    match z {
        Endpunkt::Lokal { pfad } => pfad.display().to_string(),
        Endpunkt::Ftps(f) => format!("ftps://{}{}", f.host, f.pfad),
    }
}

#[tauri::command]
pub fn konfig_ort(state: State<'_, AppState>) -> String {
    state.store.wurzel().display().to_string()
}

#[tauri::command]
pub fn strecken_liste(state: State<'_, AppState>) -> Antwort<Vec<StreckeUebersicht>> {
    let global = state.store.global_laden()?;
    let liste = state.store.strecken_liste()?;
    Ok(liste
        .into_iter()
        .map(|e| match e.ergebnis {
            Ok(s) => StreckeUebersicht {
                id: e.id,
                name: Some(s.name.clone()),
                stufen: s
                    .stufen
                    .iter()
                    .map(|st| StufeKurz {
                        id: st.id.clone(),
                        name: st.name.clone(),
                        quelle: quelle_kurz(&st.quelle),
                        ziel: ziel_kurz(&st.ziel),
                    })
                    .collect(),
                probleme: validiere_strecke(&s, &global),
                fehler: None,
            },
            Err(err) => StreckeUebersicht {
                id: e.id,
                name: None,
                stufen: vec![],
                probleme: vec![],
                fehler: Some(err.to_string()),
            },
        })
        .collect())
}

#[tauri::command]
pub fn strecke_laden(state: State<'_, AppState>, id: String) -> Antwort<Strecke> {
    Ok(state.store.strecke_laden(&id)?)
}

#[tauri::command]
pub fn strecke_pruefen(state: State<'_, AppState>, strecke: Strecke) -> Antwort<Vec<Problem>> {
    let global = state.store.global_laden()?;
    Ok(validiere_strecke(&strecke, &global))
}

/// Speichert und liefert verbleibende Warnungen.
#[tauri::command]
pub fn strecke_speichern(state: State<'_, AppState>, strecke: Strecke) -> Antwort<Vec<Problem>> {
    Ok(state.store.strecke_speichern(&strecke)?)
}

#[tauri::command]
pub fn strecke_loeschen(state: State<'_, AppState>, id: String) -> Antwort<()> {
    Ok(state.store.strecke_loeschen(&id)?)
}
