//! Befehle für das Frontend (ARCHITEKTUR Kap. 9). Hier wird nur übersetzt, nicht gerechnet.

use serde::Serialize;
use tauri::State;

use stellwerk_core::config::{validiere_strecke, Problem};
use stellwerk_core::secrets::konten_der_strecke;
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
        let kategorie = e.kategorie();
        let meldung = e.to_string();
        let probleme = match e {
            StellwerkError::Validierung(p) => p,
            _ => vec![],
        };
        Self { kategorie, meldung, probleme }
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

/// Legt eine neue Strecke an; eine vorhandene Kennung wird nie überschrieben.
#[tauri::command]
pub fn strecke_anlegen(state: State<'_, AppState>, strecke: Strecke) -> Antwort<Vec<Problem>> {
    Ok(state.store.strecke_anlegen(&strecke)?)
}

/// Speichert und liefert verbleibende Warnungen.
#[tauri::command]
pub fn strecke_speichern(state: State<'_, AppState>, strecke: Strecke) -> Antwort<Vec<Problem>> {
    Ok(state.store.strecke_speichern(&strecke)?)
}

/// Löscht die Strecke und die Passwörter ihrer FTPS-Ziele. Dateien werden nie gelöscht.
#[tauri::command]
pub fn strecke_loeschen(state: State<'_, AppState>, id: String) -> Antwort<()> {
    // Eine defekte Datei lässt sich trotzdem löschen – dann ohne Passwörter
    let konten = state.store.strecke_laden(&id).map(|s| konten_der_strecke(&s)).unwrap_or_default();
    state.store.strecke_loeschen(&id)?;
    for konto in konten {
        state.tresor.entfernen(&konto)?;
    }
    Ok(())
}

/// Legt ein Passwort im Schlüsselbund ab. Es wird nie zurückgegeben.
#[tauri::command]
pub fn passwort_speichern(state: State<'_, AppState>, konto: String, passwort: String) -> Antwort<()> {
    Ok(state.tresor.setzen(&konto, &passwort)?)
}

/// Nur ob ein Passwort hinterlegt ist – nie das Passwort selbst.
#[tauri::command]
pub fn passwort_vorhanden(state: State<'_, AppState>, konto: String) -> Antwort<bool> {
    Ok(state.tresor.vorhanden(&konto)?)
}

#[tauri::command]
pub fn passwort_entfernen(state: State<'_, AppState>, konto: String) -> Antwort<()> {
    Ok(state.tresor.entfernen(&konto)?)
}
