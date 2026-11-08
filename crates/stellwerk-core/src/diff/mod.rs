//! Vergleich Quelle ↔ Ziel (KONZEPT Kap. 4–6, ARCHITEKTUR Kap. 7 Schritt 2).
//!
//! Aus zwei Scans wird eine zeilengleiche Liste: je Pfad ein [`DiffEintrag`] mit
//! Status, beiden Seiten, greifender Regel, Warnungen und Auswahl-Vorgabe.
//! Der Vergleich verändert nichts – er ist die immer aktive Vorschau (E4).
//!
//! Regeln:
//! - **Ignorieren** → grau, nie angefasst (auch nicht gelöscht).
//! - **Schützen** → grau, nie angelegt, überschrieben oder gelöscht.
//! - **Nur anlegen** → neu, wenn am Ziel fehlend; sonst geschützt.
//! - **Ersetzen durch** → Inhalt kommt aus der Ersatzdatei der Quelle; fehlt sie: Konflikt.
//!
//! „Geändert“ bei Dateien: Größe verschieden → geändert. Gleiche Größe und gleiche
//! Zeit → gleich (schnell, ohne Lesen). Sonst entscheidet der BLAKE3-Hash.
//! Quelle auf einem Server (v0.6.5): Ihr Inhalt ist nur per Herunterladen bekannt –
//! gleiche Größe, andere Zeit heißt dann „geändert“; ohne Zeit (nur `LIST`) gilt gleiche
//! Größe als gleich, mit Hinweis „Inhalt nicht geprüft“.
//! Dateien „nur online“ werden nie gehasht, weil das den Download auslösen würde.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;

use crate::error::{Result, StellwerkError};
use crate::model::{Eintrag, EintragArt, Hash, NurOnline, RegelTyp, RelPath, Warnung};
use crate::rules::{RegelTreffer, RegelWerk};
use crate::scan::{hash_datei, ScanErgebnis};

/// Status einer Zeile (Farben: KONZEPT Kap. 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffStatus {
    /// 🟢 nur in der Quelle
    Neu,
    /// 🟡 auf beiden Seiten, Inhalt verschieden
    Geaendert,
    /// 🔴 nur im Ziel
    Loeschen,
    /// auf beiden Seiten gleich
    Gleich,
    /// ⚪ durch Regel oder bleibenden Inhalt geschützt
    Geschuetzt,
    /// ⚪ durch Regel ignoriert
    Ignoriert,
    /// nicht automatisch lösbar (Art verschieden, Symlink, Ersatz fehlt, Lesefehler)
    Konflikt,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiffEintrag {
    /// Pfad am Ziel.
    pub pfad: RelPath,
    pub status: DiffStatus,
    /// Quellseite; bei „Ersetzen durch“ der Eintrag der Ersatzdatei.
    pub quelle: Option<Eintrag>,
    pub ziel: Option<Eintrag>,
    /// Greifende Regel (Herkunft für die Oberfläche).
    pub regel: Option<RegelTreffer>,
    pub warnungen: Vec<Warnung>,
    /// Vorgabe für das Häkchen „in diesem Lauf übertragen“.
    pub ausgewaehlt: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Zusammenfassung {
    pub neu: usize,
    pub geaendert: usize,
    pub loeschen: usize,
    pub gleich: usize,
    pub geschuetzt: usize,
    pub ignoriert: usize,
    pub konflikt: usize,
    /// Zeilen mit Häkchen (anlegen, überschreiben oder löschen).
    pub ausgewaehlt: usize,
    /// Summe der Dateigrößen aller ausgewählten neuen/geänderten Dateien.
    pub bytes_uebertragen: u64,
    /// Davon „nur online“ – werden vor dem Übertragen heruntergeladen.
    pub nur_online: usize,
    pub nur_online_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Vergleich {
    pub eintraege: Vec<DiffEintrag>,
    pub zusammenfassung: Zusammenfassung,
}

#[derive(Debug, Clone, Default)]
pub struct DiffOptionen {
    /// Umgang mit Dropbox-Dateien „nur online“ (Stufen-Einstellung).
    pub nur_online: NurOnline,
    /// Inhalt auch bei gleicher Größe und Zeit prüfen (gründlich, langsamer).
    pub inhalt_immer_pruefen: bool,
    /// Die Quelle liegt auf einem Server – ihr Inhalt wird nie gelesen (v0.6.5).
    pub quelle_entfernt: bool,
}

/// Liefert Inhalts-Hashes beider Seiten – lokal aus Dateien, bei entfernten Zielen
/// aus dem Manifest (dort steht der Hash schon im Eintrag des Ziels).
pub trait InhaltPruefer {
    fn hash_quelle(&self, pfad: &RelPath) -> Result<Hash>;
    /// `None`: Inhalt des Ziels nicht bekannt – gleiche Größe gilt dann als gleich,
    /// mit Hinweis [`Warnung::InhaltNichtGeprueft`].
    fn hash_ziel(&self, pfad: &RelPath) -> Result<Option<Hash>>;
}

/// Inhalt zweier lokaler Verzeichnisse.
pub struct LokalerInhalt {
    quelle: PathBuf,
    ziel: PathBuf,
}

impl LokalerInhalt {
    pub fn new(quelle: &Path, ziel: &Path) -> Self {
        Self { quelle: quelle.to_path_buf(), ziel: ziel.to_path_buf() }
    }
}

impl InhaltPruefer for LokalerInhalt {
    fn hash_quelle(&self, pfad: &RelPath) -> Result<Hash> {
        hash_datei(&self.quelle.join(pfad.as_str()))
    }
    fn hash_ziel(&self, pfad: &RelPath) -> Result<Option<Hash>> {
        hash_datei(&self.ziel.join(pfad.as_str())).map(Some)
    }
}

/// Entferntes Ziel (FTPS): Die Quelle wird gelesen, das Ziel nie – sein Hash steht,
/// soweit bekannt, im Manifest und damit schon im Eintrag.
pub struct EntfernterInhalt {
    quelle: PathBuf,
}

impl EntfernterInhalt {
    pub fn new(quelle: &Path) -> Self {
        Self { quelle: quelle.to_path_buf() }
    }
}

impl InhaltPruefer for EntfernterInhalt {
    fn hash_quelle(&self, pfad: &RelPath) -> Result<Hash> {
        hash_datei(&self.quelle.join(pfad.as_str()))
    }
    fn hash_ziel(&self, _pfad: &RelPath) -> Result<Option<Hash>> {
        Ok(None)
    }
}

/// Eine Seite des Vergleichs, nach Pfad nachschlagbar.
struct Seite<'a> {
    eintraege: BTreeMap<&'a RelPath, &'a Eintrag>,
    ignoriert: BTreeMap<&'a RelPath, (&'a Eintrag, &'a RegelTreffer)>,
}

impl<'a> Seite<'a> {
    fn aus(scan: &'a ScanErgebnis) -> Self {
        Self {
            eintraege: scan.eintraege.iter().map(|e| (&e.pfad, e)).collect(),
            ignoriert: scan.ignoriert.iter().map(|i| (&i.eintrag.pfad, (&i.eintrag, &i.regel))).collect(),
        }
    }

    /// Eintrag einschließlich ignorierter (z. B. Ersatzdatei, die selbst ignoriert wird).
    fn irgendein(&self, p: &RelPath) -> Option<&'a Eintrag> {
        self.eintraege.get(p).copied().or_else(|| self.ignoriert.get(p).map(|(e, _)| *e))
    }
}

/// Vergleicht zwei Scans unter den Regeln einer Stufe.
pub fn vergleiche(
    quelle: &ScanErgebnis,
    ziel: &ScanErgebnis,
    regeln: &RegelWerk,
    opt: &DiffOptionen,
    inhalt: &dyn InhaltPruefer,
    abbruch: &AtomicBool,
) -> Result<Vergleich> {
    vergleiche_mit_fortschritt(quelle, ziel, regeln, opt, inhalt, abbruch, &mut |_| {})
}

/// Wie [`vergleiche`]; `fortschritt` erhält die Anzahl bereits eingeordneter Pfade.
pub fn vergleiche_mit_fortschritt(
    quelle: &ScanErgebnis,
    ziel: &ScanErgebnis,
    regeln: &RegelWerk,
    opt: &DiffOptionen,
    inhalt: &dyn InhaltPruefer,
    abbruch: &AtomicBool,
    fortschritt: &mut dyn FnMut(usize),
) -> Result<Vergleich> {
    let q = Seite::aus(quelle);
    let z = Seite::aus(ziel);
    let pfade: BTreeSet<&RelPath> =
        q.eintraege.keys().chain(q.ignoriert.keys()).chain(z.eintraege.keys()).chain(z.ignoriert.keys()).copied().collect();

    let mut eintraege = Vec::with_capacity(pfade.len());
    for pfad in pfade {
        if abbruch.load(Ordering::Relaxed) {
            return Err(StellwerkError::Abbruch);
        }
        let ign = q.ignoriert.get(pfad).or_else(|| z.ignoriert.get(pfad));
        let e = match ign {
            Some((_, regel)) => DiffEintrag {
                pfad: pfad.clone(),
                status: DiffStatus::Ignoriert,
                quelle: q.irgendein(pfad).cloned(),
                ziel: z.irgendein(pfad).cloned(),
                regel: Some((*regel).clone()),
                warnungen: vec![],
                ausgewaehlt: false,
            },
            None => einordnen(pfad, &q, &z, regeln, opt, inhalt),
        };
        eintraege.push(e);
        fortschritt(eintraege.len());
    }

    gross_klein_zwischen_seiten(&mut eintraege);
    konflikt_inhalt_abwaehlen(&mut eintraege);
    bleibendes_schuetzen(&mut eintraege);
    let zusammenfassung = zusammenfassen(&eintraege);
    Ok(Vergleich { eintraege, zusammenfassung })
}

/// Ordnet einen nicht ignorierten Pfad ein.
fn einordnen(
    pfad: &RelPath,
    q: &Seite,
    z: &Seite,
    regeln: &RegelWerk,
    opt: &DiffOptionen,
    inhalt: &dyn InhaltPruefer,
) -> DiffEintrag {
    let ze = z.eintraege.get(pfad).copied();
    let qe_eigen = q.eintraege.get(pfad).copied();
    let ist_ordner = qe_eigen.or(ze).is_some_and(Eintrag::ist_ordner);
    let regel = regeln.treffer(pfad, ist_ordner).cloned();
    let typ = regel.as_ref().map(|r| r.typ);

    let mut warnungen = Vec::new();
    // Bei „Ersetzen durch“ stammt der Inhalt aus der Ersatzdatei.
    let qe = match regel.as_ref().and_then(|r| r.ersatz.as_ref()) {
        Some(ersatz) => {
            let gefunden = q.irgendein(ersatz);
            if gefunden.is_none() {
                warnungen.push(Warnung::ErsatzFehlt { quelle: ersatz.clone() });
            }
            gefunden
        }
        None => qe_eigen,
    };
    let mut quelle = qe.cloned();
    let mut ziel = ze.cloned();

    let status = if !warnungen.is_empty() {
        DiffStatus::Konflikt
    } else if typ == Some(RegelTyp::Schuetzen) {
        DiffStatus::Geschuetzt
    } else if typ == Some(RegelTyp::NurAnlegen) {
        if quelle.is_some() && ziel.is_none() {
            DiffStatus::Neu
        } else {
            DiffStatus::Geschuetzt
        }
    } else {
        // Der Pfad stammt aus mindestens einer Seite – ohne Quelle gibt es also ein Ziel.
        match (&mut quelle, &mut ziel) {
            (Some(a), Some(b)) => beide_seiten(a, b, opt, inhalt, &mut warnungen),
            (Some(_), None) => DiffStatus::Neu,
            (None, _) => DiffStatus::Loeschen,
        }
    };

    // Warnungen beider Seiten übernehmen (ohne Doppelungen)
    for w in quelle.iter().chain(ziel.iter()).flat_map(|e| e.warnungen.iter()) {
        if !warnungen.contains(w) {
            warnungen.push(w.clone());
        }
    }
    let symlink = [&quelle, &ziel].into_iter().flatten().any(|e| e.art == EintragArt::Symlink);
    // Symlinks werden nicht verfolgt und ihre Ziele nicht verglichen → nie automatisch
    let status = if symlink
        && matches!(status, DiffStatus::Neu | DiffStatus::Geaendert | DiffStatus::Loeschen | DiffStatus::Gleich)
    {
        DiffStatus::Konflikt
    } else {
        status
    };

    let ausgewaehlt = matches!(status, DiffStatus::Neu | DiffStatus::Geaendert | DiffStatus::Loeschen)
        && !warnungen.contains(&Warnung::Konfliktkopie)
        && !(opt.nur_online == NurOnline::Ueberspringen && quelle.as_ref().is_some_and(|e| e.nur_online));

    DiffEintrag { pfad: pfad.clone(), status, quelle, ziel, regel, warnungen, ausgewaehlt }
}

/// Vergleich, wenn beide Seiten vorhanden sind. Berechnete Hashes werden eingetragen.
fn beide_seiten(
    q: &mut Eintrag,
    z: &mut Eintrag,
    opt: &DiffOptionen,
    inhalt: &dyn InhaltPruefer,
    warnungen: &mut Vec<Warnung>,
) -> DiffStatus {
    if q.art != z.art {
        warnungen.push(Warnung::ArtVerschieden);
        return DiffStatus::Konflikt;
    }
    if q.art != EintragArt::Datei {
        return DiffStatus::Gleich; // Ordner; Symlinks werden danach als Konflikt markiert
    }
    if q.groesse != z.groesse {
        return DiffStatus::Geaendert;
    }
    let zeit_gleich = q.mtime_ns.is_some() && q.mtime_ns == z.mtime_ns;
    if q.nur_online || z.nur_online {
        // Inhalt nicht prüfbar, ohne den Download auszulösen
        return if zeit_gleich && !opt.inhalt_immer_pruefen { DiffStatus::Gleich } else { DiffStatus::Geaendert };
    }
    if zeit_gleich && !opt.inhalt_immer_pruefen {
        return DiffStatus::Gleich;
    }
    if opt.quelle_entfernt {
        if q.mtime_ns.is_some() {
            return DiffStatus::Geaendert;
        }
        warnungen.push(Warnung::InhaltNichtGeprueft);
        return DiffStatus::Gleich;
    }
    // Bekannter Hash des Ziels (Manifest) zuerst – dann wird nichts am Ziel gelesen
    let hz = match z.hash {
        Some(h) => Ok(Some(h)),
        None => inhalt.hash_ziel(&z.pfad),
    };
    let paar = hz.and_then(|hz| match hz {
        Some(hz) => Ok(Some((inhalt.hash_quelle(&q.pfad)?, hz))),
        None => Ok(None),
    });
    match paar {
        Ok(None) => {
            warnungen.push(Warnung::InhaltNichtGeprueft);
            DiffStatus::Gleich
        }
        Ok(Some((hq, hz))) => {
            q.hash = Some(hq);
            z.hash = Some(hz);
            if hq == hz {
                DiffStatus::Gleich
            } else {
                DiffStatus::Geaendert
            }
        }
        Err(e) => {
            warnungen.push(Warnung::Lesefehler { meldung: e.to_string() });
            DiffStatus::Konflikt
        }
    }
}

/// `Bild.jpg` neu und `bild.jpg` zu löschen: auf case-insensitiven Dateisystemen
/// dieselbe Datei – beide Zeilen erhalten einen Hinweis aufeinander.
fn gross_klein_zwischen_seiten(eintraege: &mut [DiffEintrag]) {
    let mut zu_loeschen: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, e) in eintraege.iter().enumerate().filter(|(_, e)| e.status == DiffStatus::Loeschen) {
        zu_loeschen.entry(e.pfad.faltung()).or_default().push(i);
    }
    let paare: Vec<(usize, usize)> = eintraege
        .iter()
        .enumerate()
        .filter(|(_, e)| e.status == DiffStatus::Neu)
        .flat_map(|(i, e)| zu_loeschen.get(&e.pfad.faltung()).into_iter().flatten().map(move |&j| (i, j)))
        .collect();
    for (i, j) in paare {
        let (pi, pj) = (eintraege[i].pfad.clone(), eintraege[j].pfad.clone());
        eintraege[i].warnungen.push(Warnung::GrossKlein { mit: pj });
        eintraege[j].warnungen.push(Warnung::GrossKlein { mit: pi });
    }
}

/// Alles unterhalb eines Konflikts wird nicht vorab ausgewählt – sonst würde z. B.
/// ein Ordner geleert, der gar nicht ersetzt werden kann.
fn konflikt_inhalt_abwaehlen(eintraege: &mut [DiffEintrag]) {
    let konflikte: HashSet<RelPath> =
        eintraege.iter().filter(|e| e.status == DiffStatus::Konflikt).map(|e| e.pfad.clone()).collect();
    for e in eintraege.iter_mut() {
        if vorfahren(&e.pfad).any(|v| konflikte.contains(&v)) {
            e.ausgewaehlt = false;
        }
    }
}

/// Ein Ordner, der nur im Ziel liegt, kann nicht gelöscht werden, wenn darin
/// etwas bleibt (Geschütztes, Ignoriertes, Konflikt). Er wird dann geschützt.
fn bleibendes_schuetzen(eintraege: &mut [DiffEintrag]) {
    let bleibt = |e: &DiffEintrag| e.ziel.is_some() && (e.status != DiffStatus::Loeschen || !e.ausgewaehlt);
    let blockiert: HashSet<RelPath> = eintraege.iter().filter(|e| bleibt(e)).flat_map(|e| vorfahren(&e.pfad)).collect();
    for e in eintraege.iter_mut().filter(|e| e.status == DiffStatus::Loeschen && blockiert.contains(&e.pfad)) {
        e.status = DiffStatus::Geschuetzt;
        e.ausgewaehlt = false;
        e.warnungen.push(Warnung::EnthaeltBleibendes);
    }
}

/// Alle Vorfahren eines Pfads ohne die Wurzel, vom nächsten aufwärts.
fn vorfahren(p: &RelPath) -> impl Iterator<Item = RelPath> {
    std::iter::successors(p.parent(), RelPath::parent).filter(|v| !v.ist_wurzel())
}

/// Zählt Status und ausgewählte Bytes – auch nach jeder Änderung der Auswahl.
pub fn zusammenfassen(eintraege: &[DiffEintrag]) -> Zusammenfassung {
    let mut z = Zusammenfassung::default();
    for e in eintraege {
        *match e.status {
            DiffStatus::Neu => &mut z.neu,
            DiffStatus::Geaendert => &mut z.geaendert,
            DiffStatus::Loeschen => &mut z.loeschen,
            DiffStatus::Gleich => &mut z.gleich,
            DiffStatus::Geschuetzt => &mut z.geschuetzt,
            DiffStatus::Ignoriert => &mut z.ignoriert,
            DiffStatus::Konflikt => &mut z.konflikt,
        } += 1;
        z.ausgewaehlt += usize::from(e.ausgewaehlt);
        let quelle = e.quelle.as_ref().filter(|_| e.ausgewaehlt && e.status != DiffStatus::Loeschen);
        if let Some(q) = quelle.filter(|q| q.art == EintragArt::Datei) {
            z.bytes_uebertragen += q.groesse;
            if q.nur_online {
                z.nur_online += 1;
                z.nur_online_bytes += q.groesse;
            }
        }
    }
    z
}
