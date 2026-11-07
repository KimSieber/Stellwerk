//! Vergleich einer Stufe starten: Pfade auflösen, prüfen, beide Seiten scannen,
//! vergleichen (ARCHITEKTUR Kap. 7 Schritte 1–2). Verändert nichts (E4).

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::config::{effektive_quelle, pruefe_ordner, validiere_strecke, GlobalConfig, Problem};
use crate::diff::{vergleiche_mit_fortschritt, DiffOptionen, LokalerInhalt, Vergleich};
use crate::error::{Result, StellwerkError};
use crate::model::{Endpunkt, Quelle, Strecke};
use crate::rules::RegelWerk;
use crate::scan::{scan_lokal, ScanFehler};

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

/// Vergleicht Quelle und Ziel der Stufe `stufe_id`. Nur lokale Ordner; FTPS-Ziele
/// folgen mit dem Manifest. Fehler der Konfiguration, die diese Stufe betreffen,
/// verhindern den Vergleich – Fehler anderer Stufen nicht.
pub fn stufe_vergleichen(
    strecke: &Strecke,
    stufe_id: &str,
    global: &GlobalConfig,
    abbruch: &AtomicBool,
    fortschritt: &mut dyn FnMut(Fortschritt),
) -> Result<StufenVergleich> {
    let index = strecke
        .stufen
        .iter()
        .position(|s| s.id == stufe_id)
        .ok_or_else(|| StellwerkError::NichtGefunden(format!("Stufe „{stufe_id}“ in Strecke „{}“", strecke.id)))?;
    let stufe = &strecke.stufen[index];
    let Endpunkt::Lokal { pfad: ziel } = &stufe.ziel else {
        return Err(StellwerkError::NichtUnterstuetzt(
            "FTPS-Ziele können in dieser Version noch nicht verglichen werden".into(),
        ));
    };
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
    if let Some(p) = [quelle.as_path(), ziel.as_path()].into_iter().find(|p| !p.is_dir()) {
        return Err(StellwerkError::KeinVerzeichnis(p.to_path_buf()));
    }

    let regeln = RegelWerk::fuer_stufe(global, strecke, index)?;
    let scan = |wurzel: &Path, phase: Phase, f: &mut dyn FnMut(Fortschritt)| {
        scan_lokal(wurzel, &regeln, abbruch, &mut |anzahl| f(Fortschritt { phase, anzahl }))
    };
    let q = scan(&quelle, Phase::Quelle, fortschritt)?;
    let z = scan(ziel, Phase::Ziel, fortschritt)?;
    let opt = DiffOptionen { nur_online: stufe.nur_online, inhalt_immer_pruefen: false };
    let inhalt = LokalerInhalt::new(&quelle, ziel);
    let mut melden = |anzahl| fortschritt(Fortschritt { phase: Phase::Vergleich, anzahl });
    let vergleich = vergleiche_mit_fortschritt(&q, &z, &regeln, &opt, &inhalt, abbruch, &mut melden)?;

    let lesefehler = [(Seitenname::Quelle, q.fehler), (Seitenname::Ziel, z.fehler)]
        .into_iter()
        .flat_map(|(seite, f)| f.into_iter().map(move |ScanFehler { pfad, meldung }| Lesefehler { seite, pfad, meldung }))
        .collect();
    Ok(StufenVergleich { vergleich, quelle, ziel: ziel.clone(), lesefehler })
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
