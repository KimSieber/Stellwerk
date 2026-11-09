//! Vergleich einer Stufe starten: Pfade auflösen, prüfen, beide Seiten scannen,
//! vergleichen (ARCHITEKTUR Kap. 7 Schritte 1–2). Verändert nichts (E4).

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::config::{effektive_quelle, pruefe_ordner, validiere_strecke, GlobalConfig, Problem};
use std::collections::HashMap;

use crate::diff::{vergleiche_mit_fortschritt, DiffOptionen, InhaltPruefer, LokalerInhalt, Vergleich};
use crate::endpoint::{ServerLesen, Smb};
use crate::error::{Result, StellwerkError};
use crate::model::{Endpunkt, FtpsEndpunkt, Hash, Quelle, RelPath, SmbEndpunkt, Strecke, Stufe};
use crate::scan::hash_datei;
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

/// Gemeinsamer Anfang aller Vergleiche: Stufe finden, Quelle auflösen (lokaler Ordner
/// oder Server), Konfiguration dieser Stufe prüfen, Regeln bilden.
fn vorbereiten<'s>(strecke: &'s Strecke, stufe_id: &str, global: &GlobalConfig) -> Result<(&'s Stufe, Quelle, RegelWerk)> {
    let index = strecke
        .stufen
        .iter()
        .position(|s| s.id == stufe_id)
        .ok_or_else(|| StellwerkError::NichtGefunden(format!("Stufe „{stufe_id}“ in Strecke „{}“", strecke.id)))?;
    let stufe = &strecke.stufen[index];
    // Fehlt die Vorstufe, meldet das die Prüfung unten
    let quelle = effektive_quelle(strecke, index);
    let eigene = format!("stufen[{index}]");
    let fehler: Vec<Problem> = validiere_strecke(strecke, global)
        .into_iter()
        .chain(pruefe_ordner(strecke))
        .filter(|p| p.ist_fehler() && (!p.ort.starts_with("stufen[") || p.ort.starts_with(&eigene)))
        .collect();
    if !fehler.is_empty() {
        return Err(StellwerkError::Validierung(fehler));
    }
    let quelle = quelle.expect("ohne Fehler ist die Quelle aufgelöst");
    // Die Quelle kann der Zielordner einer anderen Stufe sein – deren Prüfung zählt oben nicht
    if let Quelle::Lokal { pfad } = &quelle {
        if !pfad.is_dir() {
            return Err(StellwerkError::KeinVerzeichnis(pfad.clone()));
        }
    }
    Ok((stufe, quelle, RegelWerk::fuer_stufe(global, strecke, index)?))
}

/// Liefert das Passwort zu einem Schlüsselbund-Verweis (`konto_ref`).
pub type Zugang<'a> = &'a dyn Fn(&str) -> Result<String>;

/// Ohne Zugang zum Schlüsselbund – für Strecken ohne Samba-Freigabe.
fn kein_zugang(konto: &str) -> Result<String> {
    Err(StellwerkError::Schluesselbund(format!("kein Passwort für {konto} verfügbar")))
}

/// Ein Ordner, der direkt gelesen und verglichen wird: lokal oder auf einer
/// Samba-Freigabe (v0.7.1). Ein Server (FTPS/SFTP) ist kein solcher Ordner – er wird über
/// den letzten bekannten Stand verglichen.
#[derive(Debug)]
pub enum Ordner {
    Lokal(PathBuf),
    Smb(Smb),
}

impl Ordner {
    /// Lokaler Ordner oder Freigabe; `None` für einen Server (FTPS/SFTP).
    pub fn aus_quelle(q: &Quelle, zugang: Zugang) -> Result<Option<Self>> {
        match q {
            Quelle::Lokal { pfad } => Ok(Some(Self::Lokal(pfad.clone()))),
            Quelle::Smb(e) => Ok(Some(Self::smb(e, zugang)?)),
            _ => Ok(None),
        }
    }

    /// Wie [`Ordner::aus_quelle`], für das Ziel einer Stufe.
    pub fn aus_ziel(z: &Endpunkt, zugang: Zugang) -> Result<Option<Self>> {
        Self::aus_quelle(&z.clone().into(), zugang)
    }

    fn smb(e: &SmbEndpunkt, zugang: Zugang) -> Result<Self> {
        Ok(Self::Smb(Smb::new(e, &zugang(&e.konto_ref)?)))
    }

    /// Ordner bzw. `smb://host:port/freigabe/pfad`.
    pub fn kennung(&self) -> PathBuf {
        match self {
            Self::Lokal(p) => p.clone(),
            Self::Smb(s) => PathBuf::from(s.kennung()),
        }
    }

    fn scan(&self, regeln: &RegelWerk, abbruch: &AtomicBool, fortschritt: &mut dyn FnMut(usize)) -> Result<ScanErgebnis> {
        match self {
            Self::Lokal(p) => scan_lokal(p, regeln, abbruch, fortschritt),
            Self::Smb(s) => s.auflisten(regeln, abbruch, fortschritt),
        }
    }

    fn hash(&self, p: &RelPath) -> Result<Hash> {
        match self {
            Self::Lokal(w) => hash_datei(&w.join(p.as_str())),
            Self::Smb(s) => s.hash(p),
        }
    }

    fn ist_smb(&self) -> bool {
        matches!(self, Self::Smb(_))
    }
}

/// Inhalte direkt aus den Ordnern; ohne Ziel-Ordner (FTPS) ist das Ziel unbekannt.
struct OrdnerInhalt<'a> {
    quelle: &'a Ordner,
    ziel: Option<&'a Ordner>,
}

impl InhaltPruefer for OrdnerInhalt<'_> {
    fn hash_quelle(&self, pfad: &RelPath) -> Result<Hash> {
        self.quelle.hash(pfad)
    }
    fn hash_ziel(&self, pfad: &RelPath) -> Result<Option<Hash>> {
        self.ziel.map(|z| z.hash(pfad)).transpose()
    }
}

/// Quellordner (lokal oder Freigabe) – eine Quelle auf einem Server (FTPS/SFTP) vergleicht
/// [`stufe_vergleichen_vom_server`].
fn ordner_quelle(q: &Quelle, zugang: Zugang) -> Result<Ordner> {
    Ordner::aus_quelle(q, zugang)?.ok_or_else(|| StellwerkError::NichtUnterstuetzt("Quelle ist ein Server (FTPS/SFTP)".into()))
}

/// Vergleicht Quelle und Ziel der Stufe `stufe_id` – lokale Ordner; für Samba-Freigaben
/// siehe [`stufe_vergleichen_mit_zugang`], für FTPS [`stufe_vergleichen_entfernt`].
/// Fehler der Konfiguration, die diese Stufe betreffen, verhindern den Vergleich –
/// Fehler anderer Stufen nicht.
pub fn stufe_vergleichen(
    strecke: &Strecke,
    stufe_id: &str,
    global: &GlobalConfig,
    abbruch: &AtomicBool,
    fortschritt: &mut dyn FnMut(Fortschritt),
) -> Result<StufenVergleich> {
    stufe_vergleichen_mit_zugang(strecke, stufe_id, global, &kein_zugang, abbruch, fortschritt)
}

/// Vergleicht zwei direkt lesbare Ordner (lokal oder Samba-Freigabe, v0.7.1); das
/// Passwort einer Freigabe liefert `zugang`.
pub fn stufe_vergleichen_mit_zugang(
    strecke: &Strecke,
    stufe_id: &str,
    global: &GlobalConfig,
    zugang: Zugang,
    abbruch: &AtomicBool,
    fortschritt: &mut dyn FnMut(Fortschritt),
) -> Result<StufenVergleich> {
    let (stufe, quelle, regeln) = vorbereiten(strecke, stufe_id, global)?;
    let quelle = ordner_quelle(&quelle, zugang)?;
    let Some(ziel) = Ordner::aus_ziel(&stufe.ziel, zugang)? else {
        return Err(StellwerkError::NichtUnterstuetzt("Server-Ziele werden gegen den letzten bekannten Stand verglichen".into()));
    };
    // Fehlt der Zielordner, meldet das schon die Prüfung in `vorbereiten`
    let scan = |o: &Ordner, phase: Phase, f: &mut dyn FnMut(Fortschritt)| {
        o.scan(&regeln, abbruch, &mut |anzahl| f(Fortschritt { phase, anzahl }))
    };
    let q = scan(&quelle, Phase::Quelle, fortschritt)?;
    let z = scan(&ziel, Phase::Ziel, fortschritt)?;
    let grob = quelle.ist_smb() || ziel.ist_smb();
    let inhalt = OrdnerInhalt { quelle: &quelle, ziel: Some(&ziel) };
    abschliessen(stufe, quelle.kennung(), ziel.kennung(), q, z, &regeln, &inhalt, (false, grob), abbruch, fortschritt)
}

/// Vergleicht eine Stufe, deren Quelle ein Server ist (FTPS v0.6.5, SFTP v0.7.2), mit ihrem Ziel:
/// lokal gescannt oder – bei einem Server als Ziel (v0.6.5b) – aus dessen letztem
/// bekanntem Stand bzw. vom Server (`ziel`). Die Quelle wird vom Server eingelesen
/// (Größe und Zeit aus `MLSD`), nie verändert und nie gelesen – „geändert“ entscheiden
/// Größe und Zeit.
pub fn stufe_vergleichen_vom_server(
    strecke: &Strecke,
    stufe_id: &str,
    global: &GlobalConfig,
    server: &dyn ServerLesen,
    ziel: Option<EntfernterStand>,
    abbruch: &AtomicBool,
    fortschritt: &mut dyn FnMut(Fortschritt),
) -> Result<EntfernterVergleich> {
    stufe_vergleichen_vom_server_mit_zugang(strecke, stufe_id, global, server, ziel, &kein_zugang, abbruch, fortschritt)
}

/// Wie [`stufe_vergleichen_vom_server`]; das Ziel darf eine Samba-Freigabe sein (v0.7.1).
#[allow(clippy::too_many_arguments)]
pub fn stufe_vergleichen_vom_server_mit_zugang(
    strecke: &Strecke,
    stufe_id: &str,
    global: &GlobalConfig,
    server: &dyn ServerLesen,
    ziel: Option<EntfernterStand>,
    zugang: Zugang,
    abbruch: &AtomicBool,
    fortschritt: &mut dyn FnMut(Fortschritt),
) -> Result<EntfernterVergleich> {
    let (stufe, quelle, regeln) = vorbereiten(strecke, stufe_id, global)?;
    let Some(quellort) = server_kennung(&quelle) else {
        return Err(StellwerkError::NichtUnterstuetzt("Quelle ist kein Server".into()));
    };
    let q = server.auflisten(&regeln, abbruch, &mut |anzahl| fortschritt(Fortschritt { phase: Phase::Quelle, anzahl }))?;
    let (zielort, z, neues_manifest, grob) = match (Ordner::aus_ziel(&stufe.ziel, zugang)?, ziel) {
        (Some(o), _) => {
            let z = o.scan(&regeln, abbruch, &mut |anzahl| fortschritt(Fortschritt { phase: Phase::Ziel, anzahl }))?;
            (o.kennung(), z, None, o.ist_smb())
        }
        (None, Some(stand)) => {
            let (z, neu) = entferntes_ziel(stand, &regeln, abbruch, fortschritt)?;
            (ziel_kennung(&stufe.ziel), z, neu, false)
        }
        (None, None) => return Err(StellwerkError::NichtUnterstuetzt("Stand des Ziel-Servers fehlt".into())),
    };
    // Wird bei entfernter Quelle nie gefragt – gleiche Größe und Zeit oder eben nicht
    let inhalt = LokalerInhalt::new(&zielort, &zielort);
    let vergleich = abschliessen(stufe, quellort, zielort, q, z, &regeln, &inhalt, (true, grob), abbruch, fortschritt)?;
    Ok(EntfernterVergleich { vergleich, neues_manifest })
}

/// Stand eines Ziel-Servers: aus dem letzten bekannten Stand oder vom Server gelesen
/// (dann zusätzlich das abgeglichene Manifest zum Speichern).
fn entferntes_ziel(
    stand: EntfernterStand,
    regeln: &RegelWerk,
    abbruch: &AtomicBool,
    fortschritt: &mut dyn FnMut(Fortschritt),
) -> Result<(ScanErgebnis, Option<Vec<ManifestEintrag>>)> {
    let (manifest, server_fehler, neues_manifest) = match stand {
        EntfernterStand::Manifest(m) => (m.to_vec(), vec![], None),
        EntfernterStand::Server { server, manifest } => {
            let server = server.auflisten(regeln, abbruch, &mut |anzahl| fortschritt(Fortschritt { phase: Phase::Ziel, anzahl }))?;
            let neu = manifest_abgleichen(manifest, &server);
            (neu.clone(), server.fehler, Some(neu))
        }
    };
    let mut z = aus_liste(manifest.iter().map(ManifestEintrag::als_eintrag).collect(), regeln);
    z.fehler.extend(server_fehler);
    Ok((z, neues_manifest))
}

/// Woher der Stand eines entfernten Ziels (FTPS oder SFTP) kommt.
pub enum EntfernterStand<'a> {
    /// Nur das Manifest – ohne Serverkontakt. Leer: noch nie verglichen.
    Manifest(&'a [ManifestEintrag]),
    /// Server einlesen (erster Vergleich, Vollabgleich). Das bisherige Manifest
    /// liefert Hashes für Dateien, deren Größe sich nicht geändert hat.
    Server { server: &'a dyn ServerLesen, manifest: &'a [ManifestEintrag] },
}

/// Ergebnis für ein entferntes Ziel; nach dem Einlesen des Servers zusätzlich das
/// abgeglichene Manifest zum Speichern.
#[derive(Debug, Clone)]
pub struct EntfernterVergleich {
    pub vergleich: StufenVergleich,
    pub neues_manifest: Option<Vec<ManifestEintrag>>,
}

/// Vergleicht eine Stufe mit Server-Ziel (FTPS, SFTP; KONZEPT Kap. 6, E11): Quelle lokal gescannt,
/// Ziel aus dem Manifest oder vom Server. Am Server wird nie etwas verändert.
pub fn stufe_vergleichen_entfernt(
    strecke: &Strecke,
    stufe_id: &str,
    global: &GlobalConfig,
    stand: EntfernterStand,
    abbruch: &AtomicBool,
    fortschritt: &mut dyn FnMut(Fortschritt),
) -> Result<EntfernterVergleich> {
    stufe_vergleichen_entfernt_mit_zugang(strecke, stufe_id, global, stand, &kein_zugang, abbruch, fortschritt)
}

/// Wie [`stufe_vergleichen_entfernt`]; die Quelle darf eine Samba-Freigabe sein (v0.7.1).
pub fn stufe_vergleichen_entfernt_mit_zugang(
    strecke: &Strecke,
    stufe_id: &str,
    global: &GlobalConfig,
    stand: EntfernterStand,
    zugang: Zugang,
    abbruch: &AtomicBool,
    fortschritt: &mut dyn FnMut(Fortschritt),
) -> Result<EntfernterVergleich> {
    let (stufe, quelle, regeln) = vorbereiten(strecke, stufe_id, global)?;
    let quelle = ordner_quelle(&quelle, zugang)?;
    if server_kennung(&stufe.ziel.clone().into()).is_none() {
        return Err(StellwerkError::NichtUnterstuetzt("Stufe hat kein Server-Ziel".into()));
    }
    let q = quelle.scan(&regeln, abbruch, &mut |anzahl| fortschritt(Fortschritt { phase: Phase::Quelle, anzahl }))?;
    let (z, neues_manifest) = entferntes_ziel(stand, &regeln, abbruch, fortschritt)?;
    let inhalt = OrdnerInhalt { quelle: &quelle, ziel: None };
    let ziel = ziel_kennung(&stufe.ziel);
    let vergleich = abschliessen(stufe, quelle.kennung(), ziel, q, z, &regeln, &inhalt, (false, false), abbruch, fortschritt)?;
    Ok(EntfernterVergleich { vergleich, neues_manifest })
}

/// Kennung eines Ziels, wie sie Vergleich und Plan verwenden: der Ordner bzw.
/// `ftps://host:port/pfad`. Ändert sie sich nach dem Vergleich, ist er veraltet.
pub fn ziel_kennung(ziel: &Endpunkt) -> PathBuf {
    match ziel {
        Endpunkt::Lokal { pfad } => pfad.clone(),
        Endpunkt::Ftps(f) => ftps_kennung(f),
        Endpunkt::Smb(e) => PathBuf::from(crate::endpoint::smb::kennung(e)),
        Endpunkt::Sftp(e) => PathBuf::from(crate::endpoint::sftp::kennung(e)),
    }
}

/// Kennung eines Servers als Quelle oder Ziel (FTPS, SFTP); `None` für andere Endpunkte.
pub fn server_kennung(q: &Quelle) -> Option<PathBuf> {
    match q {
        Quelle::Ftps(f) => Some(ftps_kennung(f)),
        Quelle::Sftp(f) => Some(PathBuf::from(crate::endpoint::sftp::kennung(f))),
        _ => None,
    }
}

/// Kennung eines Servers: `ftps://host:port/pfad` – für Ziel und Quelle (v0.6.5).
pub fn ftps_kennung(f: &FtpsEndpunkt) -> PathBuf {
    PathBuf::from(format!("ftps://{}:{}{}", f.host, f.port, f.pfad))
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
    (quelle_entfernt, zeit_grob): (bool, bool),
    abbruch: &AtomicBool,
    fortschritt: &mut dyn FnMut(Fortschritt),
) -> Result<StufenVergleich> {
    let opt = DiffOptionen { nur_online: stufe.nur_online, inhalt_immer_pruefen: false, quelle_entfernt, zeit_grob };
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
