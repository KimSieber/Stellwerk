//! Befehle für das Frontend (ARCHITEKTUR Kap. 9). Hier wird nur übersetzt, nicht gerechnet.

use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use serde_json::json;
use stellwerk_core::exec::{direkt_ausfuehren, standard_worker, ExecFortschritt, LaufErgebnis, Optionen};
use stellwerk_core::plan::{plan_erstellen as plan_erstellen_aus, Plan, PlanWarnung, PlanZusammenfassung};
use stellwerk_core::store::{jetzt_ms, JournalStatus, JournalZeile, Lauf, LaufPhase, ProtokollEbene, Protokollieren};

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::State;

use stellwerk_core::ansicht::{stufe_vergleichen, Drossel, Filter, Fortschritt, Lesefehler, Seite, VergleichsSitzung};
use stellwerk_core::diff::Zusammenfassung;
use stellwerk_core::model::RelPath;

use stellwerk_core::config::{effektive_quelle, pruefe_ordner, regel_entfernen as entfernen, regel_setzen as setzen, validiere_strecke, GlobalConfig, Problem, RegelOrt};
use stellwerk_core::secrets::konten_der_strecke;
use stellwerk_core::model::{Endpunkt, Modus, Quelle, Regel, RegelTyp, Strecke};
use stellwerk_core::StellwerkError;

use crate::state::{AppState, OffenerVergleich};

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

/// Konfigurationsprüfung und Prüfung der lokalen Ordner zusammen.
fn alle_probleme(s: &Strecke, global: &GlobalConfig) -> Vec<Problem> {
    let mut p = validiere_strecke(s, global);
    p.extend(pruefe_ordner(s));
    p
}

/// Vor dem Schreiben: Fehler (auch fehlende Ordner) verhindern das Speichern.
fn speicherbar(state: &AppState, s: &Strecke) -> Result<(), StellwerkError> {
    let p = alle_probleme(s, &state.store.global_laden()?);
    if p.iter().any(Problem::ist_fehler) {
        return Err(StellwerkError::Validierung(p));
    }
    Ok(())
}

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
                probleme: alle_probleme(&s, &global),
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

/// Globale Standardregeln (Ebene „Global“), z. B. `.DS_Store`, `.git/`.
#[tauri::command]
pub fn globale_regeln(state: State<'_, AppState>) -> Antwort<Vec<Regel>> {
    Ok(state.store.global_laden()?.regeln.standard)
}

#[tauri::command]
pub fn strecke_laden(state: State<'_, AppState>, id: String) -> Antwort<Strecke> {
    Ok(state.store.strecke_laden(&id)?)
}

#[tauri::command]
pub fn strecke_pruefen(state: State<'_, AppState>, strecke: Strecke) -> Antwort<Vec<Problem>> {
    let global = state.store.global_laden()?;
    Ok(alle_probleme(&strecke, &global))
}

/// Legt eine neue Strecke an; eine vorhandene Kennung wird nie überschrieben.
#[tauri::command]
pub fn strecke_anlegen(state: State<'_, AppState>, strecke: Strecke) -> Antwort<Vec<Problem>> {
    speicherbar(&state, &strecke)?;
    Ok(state.store.strecke_anlegen(&strecke)?)
}

/// Speichert und liefert verbleibende Warnungen.
#[tauri::command]
pub fn strecke_speichern(state: State<'_, AppState>, strecke: Strecke) -> Antwort<Vec<Problem>> {
    speicherbar(&state, &strecke)?;
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

/// Ob ein (z. B. per Drag-and-drop abgelegter) Pfad ein vorhandener Ordner ist.
/// Liest nur die Dateiart, nichts sonst.
#[tauri::command]
pub fn ist_ordner(pfad: String) -> bool {
    std::path::Path::new(&pfad).is_dir()
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

// --- Vergleich (v0.2) ---------------------------------------------------------

/// Antwort auf `vergleich_starten`; die Zeilen holt die Oberfläche mit `diff_seite`.
#[derive(Debug, Serialize)]
pub struct VergleichStart {
    pub vergleich_id: String,
    /// Aufgelöste Ordner (bei „Ziel einer Vorstufe“ deren Zielordner).
    pub quelle: String,
    pub ziel: String,
    pub zusammenfassung: Zusammenfassung,
    /// Anzahl aller Zeilen ohne Filter.
    pub zeilen: usize,
    pub lesefehler: Vec<Lesefehler>,
}

/// Scannt Quelle und Ziel einer Stufe und vergleicht sie. Verändert nichts (E4).
/// Fortschritt kommt gedrosselt über `fortschritt`. Läuft außerhalb des
/// Hauptthreads (async), damit `vergleich_abbrechen` jederzeit ankommt.
#[tauri::command]
pub async fn vergleich_starten(
    state: State<'_, AppState>,
    strecke: String,
    stufe: String,
    fortschritt: Channel<Fortschritt>,
) -> Antwort<VergleichStart> {
    let s = state.store.strecke_laden(&strecke)?;
    let global = state.store.global_laden()?;
    let schluessel = format!("{strecke}/{stufe}");
    let abbruch = state.vergleiche.anmelden(&schluessel);
    let mut drossel = Drossel::neu(Duration::from_millis(100));
    let ergebnis = stufe_vergleichen(&s, &stufe, &global, &abbruch, &mut |f| {
        if drossel.durchlassen(&f, Instant::now()) {
            // Ein geschlossenes Fenster ist kein Grund, den Vergleich abzubrechen
            let _ = fortschritt.send(f);
        }
    });
    state.vergleiche.abmelden(&schluessel, &abbruch);
    let v = ergebnis?;

    let sitzung = VergleichsSitzung::neu(v.vergleich);
    let zusammenfassung = sitzung.zusammenfassung().clone();
    let zeilen = sitzung.anzahl();
    let (quelle, ziel) = (v.quelle.display().to_string(), v.ziel.display().to_string());
    let offen = OffenerVergleich { sitzung, strecke, stufe, quelle: v.quelle, ziel: v.ziel, plan: None };
    Ok(VergleichStart {
        vergleich_id: state.vergleiche.ablegen(&schluessel, offen),
        quelle,
        ziel,
        zusammenfassung,
        zeilen,
        lesefehler: v.lesefehler,
    })
}

/// Bricht den laufenden Vergleich einer Stufe ab; `false`, wenn keiner läuft.
#[tauri::command]
pub fn vergleich_abbrechen(state: State<'_, AppState>, strecke: String, stufe: String) -> bool {
    state.vergleiche.abbrechen(&format!("{strecke}/{stufe}"))
}

/// Führt `f` mit dem offenen Vergleich `id` aus; unbekannt → „nicht gefunden“.
fn mit_vergleich<T>(
    state: &AppState,
    id: &str,
    f: impl FnOnce(&mut OffenerVergleich) -> Result<T, StellwerkError>,
) -> Antwort<T> {
    let mut sitzungen = state.vergleiche.sitzungen.lock().unwrap();
    let offen = sitzungen
        .get_mut(id)
        .ok_or_else(|| StellwerkError::NichtGefunden(format!("Vergleich „{id}“ – bitte neu vergleichen")))?;
    Ok(f(offen)?)
}

/// Wie [`mit_vergleich`], nur mit der Sitzung.
fn mit_sitzung<T>(
    state: &AppState,
    id: &str,
    f: impl FnOnce(&mut VergleichsSitzung) -> Result<T, StellwerkError>,
) -> Antwort<T> {
    mit_vergleich(state, id, |o| f(&mut o.sitzung))
}

/// Sichtbare Zeilen ab `offset` (Filter und zugeklappte Ordner wertet Rust aus).
#[tauri::command]
pub fn diff_seite(
    state: State<'_, AppState>,
    vergleich_id: String,
    offset: usize,
    limit: usize,
    filter: Filter,
    zugeklappt: Vec<RelPath>,
) -> Antwort<Seite> {
    let zu: HashSet<RelPath> = zugeklappt.into_iter().collect();
    mit_sitzung(&state, &vergleich_id, |s| Ok(s.seite(offset, limit, filter, &zu)))
}

/// Häkchen setzen/entfernen; ein Ordner wirkt auf seinen Inhalt. Liefert die neue Zusammenfassung.
#[tauri::command]
pub fn auswahl_setzen(
    state: State<'_, AppState>,
    vergleich_id: String,
    pfade: Vec<RelPath>,
    an: bool,
) -> Antwort<Zusammenfassung> {
    mit_vergleich(&state, &vergleich_id, |o| {
        // Eine andere Auswahl braucht einen neuen Plan und eine neue Bestätigung
        o.plan = None;
        o.sitzung.auswahl_setzen(&pfade, an).cloned()
    })
}

/// Gibt den Speicher eines Vergleichs frei (beim Verlassen der Ansicht).
#[tauri::command]
pub fn vergleich_schliessen(state: State<'_, AppState>, vergleich_id: String) {
    state.vergleiche.sitzungen.lock().unwrap().remove(&vergleich_id);
}

// --- Kontextmenü „Regel setzen“ (v0.2) ------------------------------------------

/// Setzt eine Regel für genau `pfad` auf der Ebene `ort`, prüft und speichert die
/// Strecke wie der Editor. Liefert verbleibende Warnungen. Danach neu vergleichen.
#[tauri::command]
pub fn regel_setzen(
    state: State<'_, AppState>,
    strecke: String,
    ort: RegelOrt,
    pfad: RelPath,
    ordner: bool,
    typ: RegelTyp,
    ersatz: Option<RelPath>,
) -> Antwort<Vec<Problem>> {
    let mut s = state.store.strecke_laden(&strecke)?;
    setzen(&mut s, &ort, &pfad, ordner, typ, ersatz.as_ref())?;
    speicherbar(&state, &s)?;
    Ok(state.store.strecke_speichern(&s)?)
}

/// Entfernt die Regel für genau `pfad` auf der Ebene `ort` und speichert.
#[tauri::command]
pub fn regel_entfernen(
    state: State<'_, AppState>,
    strecke: String,
    ort: RegelOrt,
    pfad: RelPath,
    ordner: bool,
) -> Antwort<Vec<Problem>> {
    let mut s = state.store.strecke_laden(&strecke)?;
    entfernen(&mut s, &ort, &pfad, ordner)?;
    speicherbar(&state, &s)?;
    Ok(state.store.strecke_speichern(&s)?)
}

/// Dateien der Quelle im selben Ordner wie `pfad` – Vorschläge für „Ersetzen durch …“.
#[tauri::command]
pub fn ersatz_vorschlaege(state: State<'_, AppState>, vergleich_id: String, pfad: RelPath) -> Antwort<Vec<RelPath>> {
    mit_sitzung(&state, &vergleich_id, |s| Ok(s.quelldateien_neben(&pfad)))
}

// --- Übertragen „Direkt“ (v0.3) ------------------------------------------------

/// Was vor dem Übertragen zur Bestätigung gezeigt wird.
#[derive(Debug, Serialize)]
pub struct PlanVorschau {
    pub zusammenfassung: PlanZusammenfassung,
    /// Alle Pfade, die gelöscht werden (Sammelbestätigung, E5).
    pub loeschliste: Vec<RelPath>,
    pub warnungen: Vec<PlanWarnung>,
    pub quelle: String,
    pub ziel: String,
    /// So viele Dateien werden gleichzeitig geschrieben.
    pub worker: usize,
}

/// Erstellt den Plan aus der aktuellen Auswahl und merkt ihn für `lauf_starten`.
/// Verändert nichts.
#[tauri::command]
pub fn plan_erstellen(state: State<'_, AppState>, vergleich_id: String) -> Antwort<PlanVorschau> {
    let strecken = state.store.clone();
    mit_vergleich(&state, &vergleich_id, |o| {
        let s = strecken.strecke_laden(&o.strecke)?;
        let i = s
            .stufen
            .iter()
            .position(|st| st.id == o.stufe)
            .ok_or_else(|| StellwerkError::NichtGefunden(format!("Stufe „{}“ – bitte neu vergleichen", o.stufe)))?;
        let st = &s.stufen[i];
        if st.modus == Modus::Umschalten {
            return Err(StellwerkError::NichtUnterstuetzt(
                "„Mit Umschalten“ kommt in einer späteren Version – für diese Stufe bitte „Direkt“ wählen".into(),
            ));
        }
        let gleiche_ordner = matches!(&st.ziel, Endpunkt::Lokal { pfad } if *pfad == o.ziel)
            && effektive_quelle(&s, i) == Some(Quelle::Lokal { pfad: o.quelle.clone() });
        if !gleiche_ordner {
            return Err(StellwerkError::NichtUnterstuetzt(
                "Quelle oder Ziel der Stufe wurden seit dem Vergleich geändert – bitte neu vergleichen".into(),
            ));
        }
        let plan = plan_erstellen_aus(o.sitzung.eintraege(), &st.rechte)?;
        let vorschau = PlanVorschau {
            zusammenfassung: plan.zusammenfassung.clone(),
            loeschliste: plan.loeschliste.clone(),
            warnungen: plan.warnungen.clone(),
            quelle: o.quelle.display().to_string(),
            ziel: o.ziel.display().to_string(),
            worker: standard_worker(),
        };
        o.plan = Some(plan);
        Ok(vorschau)
    })
}

#[derive(Debug, Serialize)]
pub struct LaufAntwort {
    pub lauf_id: String,
    pub ergebnis: LaufErgebnis,
}

/// Führt den zuletzt bestätigten Plan aus („Direkt“). Schreibt nur ins Ziel (E3).
/// Löschungen müssen ausdrücklich bestätigt sein (E5). Jede Operation steht im
/// Journal, Beginn, Fehler und Ende im Protokoll (E8). Danach muss neu verglichen werden.
#[tauri::command]
pub async fn lauf_starten(
    state: State<'_, AppState>,
    vergleich_id: String,
    loeschen_bestaetigt: bool,
    bei_fehler_stoppen: bool,
    fortschritt: Channel<ExecFortschritt>,
) -> Antwort<LaufAntwort> {
    let (plan, strecke, stufe, quelle, ziel) = mit_vergleich(&state, &vergleich_id, |o| {
        // Der Plan gilt genau einmal – ein zweiter Start braucht eine neue Bestätigung
        let plan = o.plan.take().ok_or_else(|| {
            StellwerkError::NichtBestaetigt("Übertragung – bitte „Übertragen …“ erneut öffnen und bestätigen".into())
        })?;
        Ok((plan, o.strecke.clone(), o.stufe.clone(), o.quelle.clone(), o.ziel.clone()))
    })?;
    if plan.hat_loeschungen() && !loeschen_bestaetigt {
        return Err(StellwerkError::NichtBestaetigt(format!("{} Löschungen", plan.loeschliste.len())).into());
    }
    let schluessel = format!("{strecke}/{stufe}");
    let abbruch = state
        .laeufe
        .anmelden(&schluessel)
        .ok_or_else(|| StellwerkError::Vorhanden(format!("Übertragung für „{schluessel}“ läuft bereits")))?;
    let ergebnis = uebertragen(&state, &plan, &strecke, &stufe, &quelle, &ziel, bei_fehler_stoppen, &abbruch, &fortschritt);
    state.laeufe.abmelden(&schluessel);
    Ok(ergebnis?)
}

#[allow(clippy::too_many_arguments)]
fn uebertragen(
    state: &AppState,
    plan: &Plan,
    strecke: &str,
    stufe: &str,
    quelle: &Path,
    ziel: &Path,
    bei_fehler_stoppen: bool,
    abbruch: &AtomicBool,
    fortschritt: &Channel<ExecFortschritt>,
) -> Result<LaufAntwort, StellwerkError> {
    let db = &state.datenbank;
    let worker = standard_worker();
    let lauf_id = db.lock().unwrap().lauf_anlegen(strecke, stufe, Modus::Direkt, plan, jetzt_ms())?;
    let ort = (strecke, stufe, lauf_id.as_str());
    {
        let db = db.lock().unwrap();
        db.protokollieren(&eintrag(
            ort,
            ProtokollEbene::Info,
            "lauf_gestartet",
            None,
            json!({
                "modus": "direkt", "quelle": quelle, "ziel": ziel, "worker": worker,
                "bei_fehler_stoppen": bei_fehler_stoppen, "plan": plan.zusammenfassung,
            }),
        ))?;
        db.lauf_phase(&lauf_id, LaufPhase::Laeuft, jetzt_ms(), None)?;
    }

    let opt = Optionen { worker, bei_fehler_stoppen, fortschritt_abstand: Duration::from_millis(100) };
    let ergebnis = direkt_ausfuehren(
        plan,
        quelle,
        ziel,
        &lauf_id,
        &opt,
        abbruch,
        &mut |op, status, meldung| {
            let db = db.lock().unwrap();
            db.journal_setzen(&lauf_id, op.nr, status, meldung, jetzt_ms())?;
            if status == JournalStatus::Fehler {
                let details = json!({ "op": op.art, "meldung": meldung });
                db.protokollieren(&eintrag(ort, ProtokollEbene::Fehler, "operation_fehler", Some(op.pfad.as_str()), details))?;
            }
            Ok(())
        },
        // Ein geschlossenes Fenster ist kein Grund, die Übertragung abzubrechen
        &mut |f| drop(fortschritt.send(f)),
    );

    let (phase, ebene) = if ergebnis.abgebrochen {
        (LaufPhase::Abgebrochen, ProtokollEbene::Warnung)
    } else if ergebnis.vollstaendig() {
        (LaufPhase::Fertig, ProtokollEbene::Info)
    } else {
        (LaufPhase::Fertig, ProtokollEbene::Fehler)
    };
    let zusammen = json!(ergebnis);
    let db = db.lock().unwrap();
    db.lauf_phase(&lauf_id, phase, jetzt_ms(), Some(&zusammen))?;
    db.protokollieren(&eintrag(ort, ebene, "lauf_beendet", None, zusammen))?;
    Ok(LaufAntwort { lauf_id, ergebnis })
}

/// Protokolleintrag zu einem Lauf; `ort` = (Strecke, Stufe, Lauf).
fn eintrag<'a>(
    ort: (&'a str, &'a str, &'a str),
    ebene: ProtokollEbene,
    aktion: &'a str,
    pfad: Option<&'a str>,
    details: serde_json::Value,
) -> Protokollieren<'a> {
    Protokollieren {
        zeit: jetzt_ms(),
        ebene: Some(ebene),
        strecke_id: Some(ort.0),
        stufe_id: Some(ort.1),
        lauf_id: Some(ort.2),
        aktion,
        pfad,
        details: Some(details),
    }
}

/// Läufe einer Stufe, neueste zuerst (Protokoll in der Streckenansicht).
#[tauri::command]
pub fn laeufe(state: State<'_, AppState>, strecke: String, stufe: String, limit: usize) -> Antwort<Vec<Lauf>> {
    Ok(state.datenbank.lock().unwrap().laeufe(&strecke, &stufe, limit)?)
}

/// Jeder Schritt eines Laufs in Plan-Reihenfolge; unbekannter Lauf → leer.
#[tauri::command]
pub fn lauf_journal(state: State<'_, AppState>, lauf_id: String) -> Antwort<Vec<JournalZeile>> {
    Ok(state.datenbank.lock().unwrap().journal(&lauf_id)?)
}

/// Bricht die laufende Übertragung einer Stufe ab; `false`, wenn keine läuft.
#[tauri::command]
pub fn lauf_abbrechen(state: State<'_, AppState>, strecke: String, stufe: String) -> bool {
    state.laeufe.abbrechen(&format!("{strecke}/{stufe}"))
}
