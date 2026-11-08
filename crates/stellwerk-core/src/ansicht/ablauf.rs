//! Vergleich einer Stufe starten: Pfade auflösen, prüfen, beide Seiten scannen,
//! vergleichen (ARCHITEKTUR Kap. 7 Schritte 1–2). Verändert nichts (E4).

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::config::{effektive_quelle, pruefe_ordner, validiere_strecke, GlobalConfig, Problem};
use std::collections::HashMap;

use crate::diff::{vergleiche_mit_fortschritt, DiffOptionen, EntfernterInhalt, InhaltPruefer, LokalerInhalt, Vergleich};
use crate::endpoint::Ftps;
use crate::error::{Result, StellwerkError};
use crate::model::{Endpunkt, Quelle, RelPath, Strecke, Stufe};
use crate::rules::RegelWerk;
use crate::scan::{aus_liste, scan_lokal, ScanErgebnis, ScanFehler};
use crate::store::ManifestEintrag;

/// Arbeitsschritt eines laufenden Vergleichs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Quelle wird gelesen
    Quelle,
    /// Ziel wird gelesen
    Ziel,
    /// Einträge werden verglichen (ggf. Inhalte gehasht)
    Vergleich,
}

/// Fortschrittsmeldung: Anzahl bisher bearbeiteter Einträge der Phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Fortschritt {
    pub phase: Phase,
    pub anzahl: usize,
}

/// Seite, auf der ein Lesefehler auftrat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Seitenname {
    Quelle,
    Ziel,
}

/// Nicht lesbarer Eintrag beim Scan – der Vergleich läuft trotzdem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Lesefehler {
    pub seite: Seitenname,
    pub pfad: String,
    pub meldung: String,
}

#[derive(Debug, Clone)]
pub struct StufenVergleich {
    pub vergleich: Vergleich,
    /// Aufgelöster Quellordner (bei „Ziel einer Vorstufe“ deren Zielordner).
    pub quelle: PathBuf,
    pub ziel: PathBuf,
    pub lesefehler: Vec<Lesefehler>,
}

/// Gemeinsamer Anfang beider Vergleiche: Stufe finden, Quelle auflösen, Konfiguration
/// dieser Stufe prüfen, Regeln bilden.
fn vorbereiten<'s>(strecke: &'s Strecke, stufe_id: &str, global: &GlobalConfig) -> Result<(usize, &'s Stufe, PathBuf, RegelWerk)> {
    let index = strecke
        .stufen
        .iter()
        .position(|s| s.id == stufe_id)
        .ok_or_else(|| StellwerkError::NichtGefunden(format!("Stufe „{stufe_id}“ in Strecke „{}“", strecke.id)))?;
    let stufe = &strecke.stufen[index];
    let Some(Quelle::Lokal { pfad: quelle }) = effektive_quelle(strecke, index) else {
        return Err(StellwerkError::NichtUnterstuetzt("Quelle ist kein lokaler Ordner".into()));
    };
    let eigene = format!("stufen[{index}]");
    let fehler: Vec<Problem> = validiere_strecke(strecke, global)
        .into_iter()
        .chain(pruefe_ordner(strecke))
        .filter(|p| p.ist_fehler() && (!p.ort.starts_with("stufen[") || p.ort.starts_with(&eigene)))
        .collect();
    if !fehler.is_empty() {
        return Err(StellwerkError::Validierung(fehler));
    }
    // Die Quelle kann der Zielordner einer anderen Stufe sein – deren Prüfung zählt oben nicht
    if !quelle.is_dir() {
        return Err(StellwerkError::KeinVerzeichnis(quelle));
    }
    Ok((index, stufe, quelle, RegelWerk::fuer_stufe(global, strecke, index)?))
}

/// Vergleicht Quelle und Ziel der Stufe `stufe_id` – nur lokale Ziele; für FTPS
/// siehe [`stufe_vergleichen_entfernt`]. Fehler der Konfiguration, die diese Stufe
/// betreffen, verhindern den Vergleich – Fehler anderer Stufen nicht.
pub fn stufe_vergleichen(
    strecke: &Strecke,
    stufe_id: &str,
    global: &GlobalConfig,
    abbruch: &AtomicBool,
    fortschritt: &mut dyn FnMut(Fortschritt),
) -> Result<StufenVergleich> {
    let (_, stufe, quelle, regeln) = vorbereiten(strecke, stufe_id, global)?;
    let Endpunkt::Lokal { pfad: ziel } = &stufe.ziel else {
        return Err(StellwerkError::NichtUnterstuetzt("FTPS-Ziele werden gegen das Manifest verglichen".into()));
    };
    // Fehlt der Zielordner, meldet das schon die Prüfung in `vorbereiten`
    let scan = |wurzel: &Path, phase: Phase, f: &mut dyn FnMut(Fortschritt)| {
        scan_lokal(wurzel, &regeln, abbruch, &mut |anzahl| f(Fortschritt { phase, anzahl }))
    };
    let q = scan(&quelle, Phase::Quelle, fortschritt)?;
    let z = scan(ziel, Phase::Ziel, fortschritt)?;
    let inhalt = LokalerInhalt::new(&quelle, ziel);
    abschliessen(stufe, quelle, ziel.clone(), q, z, &regeln, &inhalt, abbruch, fortschritt)
}

/// Woher der Stand eines entfernten Ziels (FTPS) kommt.
pub enum EntfernterStand<'a> {
    /// Nur das Manifest – ohne Serverkontakt. Leer: noch nie verglichen.
    Manifest(&'a [ManifestEintrag]),
    /// Server einlesen (erster Vergleich, Vollabgleich). Das bisherige Manifest
    /// liefert Hashes für Dateien, deren Größe sich nicht geändert hat.
    Server { ftps: &'a Ftps, manifest: &'a [ManifestEintrag] },
}

/// Ergebnis für ein entferntes Ziel; nach dem Einlesen des Servers zusätzlich das
/// abgeglichene Manifest zum Speichern.
#[derive(Debug, Clone)]
pub struct EntfernterVergleich {
    pub vergleich: StufenVergleich,
    pub neues_manifest: Option<Vec<ManifestEintrag>>,
}

/// Vergleicht eine Stufe mit FTPS-Ziel (KONZEPT Kap. 6, E11): Quelle lokal gescannt,
/// Ziel aus dem Manifest oder vom Server. Am Server wird nie etwas verändert.
pub fn stufe_vergleichen_entfernt(
    strecke: &Strecke,
    stufe_id: &str,
    global: &GlobalConfig,
    stand: EntfernterStand,
    abbruch: &AtomicBool,
    fortschritt: &mut dyn FnMut(Fortschritt),
) -> Result<EntfernterVergleich> {
    let (_, stufe, quelle, regeln) = vorbereiten(strecke, stufe_id, global)?;
    let Endpunkt::Ftps(f) = &stufe.ziel else {
        return Err(StellwerkError::NichtUnterstuetzt("Stufe hat kein FTPS-Ziel".into()));
    };
    let q = scan_lokal(&quelle, &regeln, abbruch, &mut |anzahl| fortschritt(Fortschritt { phase: Phase::Quelle, anzahl }))?;
    let (manifest, server_fehler, neues_manifest) = match stand {
        EntfernterStand::Manifest(m) => (m.to_vec(), vec![], None),
        EntfernterStand::Server { ftps, manifest } => {
            let server = ftps.auflisten(&regeln, abbruch, &mut |anzahl| fortschritt(Fortschritt { phase: Phase::Ziel, anzahl }))?;
            let neu = manifest_abgleichen(manifest, &server);
            (neu.clone(), server.fehler, Some(neu))
        }
    };
    let mut z = aus_liste(manifest.iter().map(ManifestEintrag::als_eintrag).collect(), &regeln);
    z.fehler.extend(server_fehler);
    let inhalt = EntfernterInhalt::new(&quelle);
    let ziel = PathBuf::from(format!("ftps://{}:{}{}", f.host, f.port, f.pfad));
    let vergleich = abschliessen(stufe, quelle, ziel, q, z, &regeln, &inhalt, abbruch, fortschritt)?;
    Ok(EntfernterVergleich { vergleich, neues_manifest })
}

/// Stand des Servers mit dem bisherigen Manifest zusammenführen (nach Pfad sortiert): Hat sich Art und
/// Größe nicht geändert, bleiben Hash und Zeit des Manifests; sonst ist der Inhalt
/// unbekannt. Was der Server nicht mehr hat, fällt heraus.
pub fn manifest_abgleichen(alt: &[ManifestEintrag], server: &ScanErgebnis) -> Vec<ManifestEintrag> {
    let alt: HashMap<&RelPath, &ManifestEintrag> = alt.iter().map(|m| (&m.pfad, m)).collect();
    let mut neu: Vec<ManifestEintrag> = server
        .eintraege
        .iter()
        .chain(server.ignoriert.iter().map(|i| &i.eintrag))
        .map(|e| {
            let ist_ordner = e.ist_ordner();
            match alt.get(&e.pfad).filter(|m| m.ist_ordner == ist_ordner && m.groesse == e.groesse) {
                Some(m) => (*m).clone(),
                None => ManifestEintrag { pfad: e.pfad.clone(), ist_ordner, groesse: e.groesse, hash: None, mtime_ns: None, lauf_id: None },
            }
        })
        .collect();
    neu.sort_by(|a, b| a.pfad.cmp(&b.pfad));
    neu
}

#[allow(clippy::too_many_arguments)]
fn abschliessen(
    stufe: &Stufe,
    quelle: PathBuf,
    ziel: PathBuf,
    q: ScanErgebnis,
    z: ScanErgebnis,
    regeln: &RegelWerk,
    inhalt: &dyn InhaltPruefer,
    abbruch: &AtomicBool,
    fortschritt: &mut dyn FnMut(Fortschritt),
) -> Result<StufenVergleich> {
    let opt = DiffOptionen { nur_online: stufe.nur_online, inhalt_immer_pruefen: false };
    let mut melden = |anzahl| fortschritt(Fortschritt { phase: Phase::Vergleich, anzahl });
    let vergleich = vergleiche_mit_fortschritt(&q, &z, regeln, &opt, inhalt, abbruch, &mut melden)?;
    let lesefehler = [(Seitenname::Quelle, q.fehler), (Seitenname::Ziel, z.fehler)]
        .into_iter()
        .flat_map(|(seite, f)| f.into_iter().map(move |ScanFehler { pfad, meldung }| Lesefehler { seite, pfad, meldung }))
        .collect();
    Ok(StufenVergleich { vergleich, quelle, ziel, lesefehler })
}

/// Begrenzt Fortschrittsmeldungen auf eine je `abstand` (≈ 10/s, ARCHITEKTUR Kap. 9).
/// Ein Wechsel der Phase wird immer gemeldet, damit die Oberfläche ihn sofort zeigt.
pub struct Drossel {
    abstand: Duration,
    zuletzt: Option<(Instant, Phase)>,
}

impl Drossel {
    pub fn neu(abstand: Duration) -> Self {
        Self { abstand, zuletzt: None }
    }

    /// Ob die Meldung zum Zeitpunkt `jetzt` weitergegeben werden soll.
    pub fn durchlassen(&mut self, f: &Fortschritt, jetzt: Instant) -> bool {
        let ok = match self.zuletzt {
            Some((t, phase)) => phase != f.phase || jetzt.duration_since(t) >= self.abstand,
            None => true,
        };
        if ok {
            self.zuletzt = Some((jetzt, f.phase));
        }
        ok
    }
}
