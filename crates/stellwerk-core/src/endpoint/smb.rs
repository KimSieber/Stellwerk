//! Samba-Freigabe (SMB 2/3) als Endpunkt – ohne eingehängtes Laufwerk (v0.7.1).
//!
//! - Anmeldung mit Benutzer und Passwort aus dem Schlüsselbund (NTLM), über die
//!   synchrone Variante der Bibliothek `smb` (reines Rust).
//! - Verhält sich wie ein lokaler Ordner: Einlesen, Vergleichen und Inhalte prüfen
//!   direkt auf der Freigabe – ohne „letzten bekannten Stand“.
//! - Zeiten speichert SMB auf 100 ns genau; der Vergleich berücksichtigt das.
//! - Als Ziel: Temporärdatei, Zeit der Quelle setzen, dann umbenennen (mit Ersetzen).
//!   Unix-Rechte kennt SMB nicht – sie werden nicht gesetzt.
//! - Jeder Worker erhält eine eigene Verbindung; gelesen wird über eine gemeinsame.

use std::io::Read;
use std::path::Path;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use smb::binrw_util::prelude::FileTime;
use smb::{GetLen, ResourceHandle};
use smb::{
    Client, ClientConfig, ConnectionConfig, CreateOptions, DirAccessMask, File, FileAccessMask, FileAttributes, FileBasicInformation,
    FileCreateArgs, FileDirectoryInformation, FileDispositionInformation, FileRenameInformation, Resource, Status, UncPath,
};

use super::{kopieren, temp_name, Geschrieben, QuellDatei, QuellEndpunkt, ZielEndpunkt, ZielVerbindung};
use crate::error::{Result, StellwerkError};
use crate::model::{Eintrag, EintragArt, Hash, RegelTyp, RelPath, SmbEndpunkt, Warnung};
use crate::rules::RegelWerk;
use crate::scan::{aus_liste, ist_konfliktkopie, zeit_ns, ScanErgebnis, ScanFehler};

/// Gleichzeitige Verbindungen beim Schreiben auf eine Freigabe (im LAN unkritisch).
pub const SMB_WORKER: usize = 4;

/// Wartezeit auf den Server, bevor eine Operation als gescheitert gilt.
const ZEITLIMIT: Duration = Duration::from_secs(20);

/// Eine Samba-Freigabe mit Zugangsdaten.
pub struct Smb {
    e: SmbEndpunkt,
    passwort: String,
    /// Gemeinsame Verbindung zum Lesen (Einlesen, Inhalte, Quelle) – bei Bedarf aufgebaut.
    lesen: Mutex<Option<Arc<Sitzung>>>,
}

impl std::fmt::Debug for Smb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Ohne Passwort
        write!(f, "Smb({})", kennung(&self.e))
    }
}

/// `smb://host:port/freigabe/pfad` – Kennung für Vergleich und Plan.
pub fn kennung(e: &SmbEndpunkt) -> String {
    let pfad = e.pfad.trim_end_matches('/');
    format!("smb://{}:{}/{}{}", e.host, e.port, e.freigabe, pfad)
}

/// Ergebnis von „Verbindung testen“.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SmbVerbindungstest {
    /// `smb://host:port/freigabe/pfad`
    pub ordner: String,
    /// Einträge im Ordner der Stufe (ohne Unterordner).
    pub eintraege: usize,
}

/// Eine angemeldete Verbindung zur Freigabe.
struct Sitzung {
    client: Client,
    wurzel: UncPath,
}

impl Drop for Sitzung {
    fn drop(&mut self) {
        let _ = self.client.close();
    }
}

/// Schließt eine geöffnete Ressource spätestens beim Verlassen des Gültigkeitsbereichs
/// (die synchrone Bibliothek schließt beim Verwerfen nicht selbst).
struct Offen<T: std::ops::Deref<Target = ResourceHandle>>(T);

impl<T: std::ops::Deref<Target = ResourceHandle>> Drop for Offen<T> {
    fn drop(&mut self) {
        // Schon geschlossen ist kein Fehler
        let _ = self.0.close();
    }
}

impl Smb {
    pub fn new(e: &SmbEndpunkt, passwort: &str) -> Self {
        Self { e: e.clone(), passwort: passwort.to_string(), lesen: Mutex::new(None) }
    }

    pub fn kennung(&self) -> String {
        kennung(&self.e)
    }

    /// Meldet sich an der Freigabe an und prüft, dass der Ordner der Stufe da ist.
    fn anmelden(&self) -> Result<Sitzung> {
        let wurzel_text = format!(r"\\{}\{}", self.e.host, self.e.freigabe);
        let wurzel = mit_ort(&self.kennung(), UncPath::from_str(&wurzel_text))?;
        let connection = ConnectionConfig { port: Some(self.e.port), timeout: Some(ZEITLIMIT), ..Default::default() };
        let client = Client::new(ClientConfig { dfs: false, connection, ..Default::default() });
        mit_ort(&self.kennung(), client.share_connect(&wurzel, &self.e.benutzer, self.passwort.clone()))?;
        Ok(Sitzung { client, wurzel })
    }

    /// Die gemeinsame Verbindung zum Lesen.
    fn lesend(&self) -> Result<Arc<Sitzung>> {
        let mut s = self.lesen.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(s) = s.as_ref() {
            return Ok(s.clone());
        }
        let neu = Arc::new(self.anmelden()?);
        *s = Some(neu.clone());
        Ok(neu)
    }

    /// Pfad innerhalb der Freigabe (mit `\`), ausgehend vom Ordner der Stufe.
    fn innen(&self, p: &RelPath) -> String {
        self.e
            .pfad
            .split('/')
            .chain(p.as_str().split('/'))
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\\")
    }

    fn ort(&self, p: &RelPath) -> String {
        format!("{}/{}", self.kennung(), p.as_str())
    }

    /// Liest den Ordner der Stufe rekursiv ein. Ignorierte Ordner werden nicht
    /// durchlaufen; nicht lesbare Unterordner werden gemeldet, nicht abgebrochen.
    pub fn auflisten(&self, regeln: &RegelWerk, abbruch: &AtomicBool, fortschritt: &mut dyn FnMut(usize)) -> Result<ScanErgebnis> {
        let s = self.lesend()?;
        let mut liste = Vec::new();
        let mut fehler = Vec::new();
        let mut offen = vec![RelPath::wurzel()];
        while let Some(ordner) = offen.pop() {
            if abbruch.load(Ordering::Relaxed) {
                return Err(StellwerkError::Abbruch);
            }
            let eintraege = match self.ordner_lesen(&s, &ordner) {
                Ok(e) => e,
                Err(e) if ordner.ist_wurzel() => return Err(e),
                Err(e) => {
                    fehler.push(ScanFehler { pfad: ordner.to_string(), meldung: format!("Ordner nicht lesbar: {e}") });
                    continue;
                }
            };
            let (gut, schlecht): (Vec<_>, Vec<_>) = eintraege.into_iter().partition(std::result::Result::is_ok);
            fehler.extend(schlecht.into_iter().filter_map(std::result::Result::err));
            for eintrag in gut.into_iter().flatten() {
                let ignoriert = regeln.treffer(&eintrag.pfad, true).is_some_and(|t| t.typ == RegelTyp::Ignorieren);
                if eintrag.art == EintragArt::Ordner && !ignoriert {
                    offen.push(eintrag.pfad.clone());
                }
                liste.push(eintrag);
                fortschritt(liste.len());
            }
        }
        let mut erg = aus_liste(liste, regeln);
        erg.fehler.extend(fehler);
        Ok(erg)
    }

    /// Einträge eines Ordners (ohne `.` und `..`).
    fn ordner_lesen(&self, s: &Sitzung, ordner: &RelPath) -> Result<Vec<std::result::Result<Eintrag, ScanFehler>>> {
        let ort = self.ort(ordner);
        let zugriff = DirAccessMask::new().with_list_directory(true).with_read_attributes(true);
        let dir = oeffnen(s, &self.innen(ordner), &FileCreateArgs::make_open_existing(zugriff.into()), &ort)?;
        let Resource::Directory(dir) = dir else {
            return Err(StellwerkError::KeinVerzeichnis(ort.into()));
        };
        let dir = Offen(dir);
        let mut erg = Vec::new();
        for roh in mit_ort(&ort, dir.0.query::<FileDirectoryInformation>("*"))? {
            let info = mit_ort(&ort, roh)?;
            let name = info.file_name.to_string();
            if name == "." || name == ".." {
                continue;
            }
            erg.push(eintrag_aus(ordner, &name, &info));
        }
        Ok(erg)
    }

    /// „Verbindung testen“ (v0.7.1b): anmelden und den Ordner der Stufe lesen –
    /// über eine eigene, neue Verbindung. Verändert nichts.
    pub fn verbindung_testen(&self) -> Result<SmbVerbindungstest> {
        let s = self.anmelden()?;
        let eintraege = self.ordner_lesen(&s, &RelPath::wurzel())?.len();
        Ok(SmbVerbindungstest { ordner: self.kennung(), eintraege })
    }

    /// BLAKE3-Hash einer Datei – liest sie einmal über die gemeinsame Verbindung.
    pub fn hash(&self, p: &RelPath) -> Result<Hash> {
        let mut q = self.oeffnen(p)?;
        let mut h = blake3::Hasher::new();
        h.update_reader(&mut q.inhalt).map_err(verbindung(&self.ort(p)))?;
        Ok(Hash(*h.finalize().as_bytes()))
    }
}

/// Einen Verzeichniseintrag in einen [`Eintrag`] übersetzen.
fn eintrag_aus(ordner: &RelPath, name: &str, info: &FileDirectoryInformation) -> std::result::Result<Eintrag, ScanFehler> {
    let pfad = ordner
        .join(name)
        .map_err(|e| ScanFehler { pfad: format!("{}/{name}", ordner.as_str()), meldung: e.to_string() })?;
    let attr = info.file_attributes;
    let art = if attr.reparse_point() {
        EintragArt::Symlink
    } else if attr.directory() {
        EintragArt::Ordner
    } else {
        EintragArt::Datei
    };
    let warnungen = [
        (art == EintragArt::Symlink).then(|| Warnung::Symlink { ziel: String::new() }),
        ist_konfliktkopie(name).then_some(Warnung::Konfliktkopie),
    ]
    .into_iter()
    .flatten()
    .collect();
    Ok(Eintrag {
        groesse: if art == EintragArt::Datei { info.end_of_file } else { 0 },
        mtime_ns: zeit_ns(SystemTime::from(info.last_write_time)),
        pfad,
        art,
        nur_online: false,
        hash: None,
        warnungen,
    })
}

/// Öffnet eine Ressource relativ zur Freigabe.
fn oeffnen(s: &Sitzung, innen: &str, args: &FileCreateArgs, ort: &str) -> Result<Resource> {
    let unc = if innen.is_empty() { s.wurzel.clone() } else { s.wurzel.clone().with_path(innen) };
    mit_ort(ort, s.client.create_file(&unc, args))
}

/// Zeitpunkt als SMB-Zeit (100 ns seit 1601).
fn filetime(t: SystemTime) -> FileTime {
    const SEIT_1601: i128 = 116_444_736_000_000_000;
    let ns = zeit_ns(t).map_or(0, i128::from);
    FileTime::from(u64::try_from(SEIT_1601 + ns.div_euclid(100)).unwrap_or(0))
}

/// Fehler der Bibliothek mit Ort übersetzen.
fn mit_ort<T>(ort: &str, r: smb::Result<T>) -> Result<T> {
    r.map_err(|e| fehler(ort, e))
}

/// Ein-/Ausgabefehler beim Übertragen als Verbindungsfehler mit Ort.
fn verbindung(ort: &str) -> impl Fn(std::io::Error) -> StellwerkError + '_ {
    move |e| StellwerkError::Verbindung(format!("{ort}: {e}"))
}

/// Statuscode einer Fehlerantwort des Servers.
fn status(e: &smb::Error) -> Option<u32> {
    match e {
        smb::Error::UnexpectedMessageStatus(s) | smb::Error::ReceivedErrorMessage(s, _) => Some(*s),
        _ => None,
    }
}

/// Übersetzt Fehler der Bibliothek in Stellwerk-Fehler (Meldungen ohne Passwort).
fn fehler(ort: &str, e: smb::Error) -> StellwerkError {
    const ANMELDUNG: u32 = Status::LogonFailure as u32;
    const FEHLT: u32 = Status::ObjectNameNotFound as u32;
    const PFAD_FEHLT: u32 = Status::ObjectPathNotFound as u32;
    const FREIGABE_FEHLT: u32 = Status::BadNetworkName as u32;
    const VERWEIGERT: u32 = Status::AccessDenied as u32;
    const VORHANDEN: u32 = Status::ObjectNameCollision as u32;
    match status(&e) {
        Some(ANMELDUNG) => StellwerkError::Anmeldung(format!("{ort}: Benutzer oder Passwort falsch")),
        Some(FEHLT | PFAD_FEHLT) => StellwerkError::NichtGefunden(ort.to_string()),
        Some(FREIGABE_FEHLT) => StellwerkError::NichtGefunden(format!("Freigabe {ort}")),
        Some(VERWEIGERT) => StellwerkError::KeineRechte(format!("{ort}: Zugriff verweigert")),
        Some(VORHANDEN) => StellwerkError::Vorhanden(format!("{ort} – am Ziel inzwischen vorhanden, bitte neu vergleichen")),
        _ => StellwerkError::Verbindung(format!("{ort}: {e}")),
    }
}

/// Eine geöffnete Datei der Freigabe als Lesestrom; hält die Verbindung.
struct Leser {
    datei: Offen<File>,
    _sitzung: Arc<Sitzung>,
}

impl Read for Leser {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.datei.0.read(buf)
    }
}

impl QuellEndpunkt for Smb {
    fn oeffnen(&self, p: &RelPath) -> Result<QuellDatei> {
        let s = self.lesend()?;
        let ort = self.ort(p);
        let zugriff = FileAccessMask::new().with_generic_read(true);
        let datei = match oeffnen(&s, &self.innen(p), &FileCreateArgs::make_open_existing(zugriff), &ort)? {
            Resource::File(d) => d,
            andere => {
                // Ordner o. Ä. – sofort wieder schließen
                drop(OffenRes(andere));
                return Err(StellwerkError::NichtUnterstuetzt(format!("{ort} ist keine Datei")));
            }
        };
        let datei = Offen(datei);
        let info: FileBasicInformation = mit_ort(&ort, datei.0.query_info())?;
        let groesse = mit_ort(&ort, datei.0.get_len())?;
        Ok(QuellDatei {
            groesse,
            geaendert: SystemTime::from(info.last_write_time),
            modus: None,
            inhalt: Box::new(Leser { datei, _sitzung: s }),
        })
    }
}

/// Schließt eine Ressource beliebiger Art.
struct OffenRes(Resource);

impl Drop for OffenRes {
    fn drop(&mut self) {
        let _ = handle(&self.0).close();
    }
}

impl ZielEndpunkt for Smb {
    fn verbinden(&self) -> Result<Box<dyn ZielVerbindung + '_>> {
        Ok(Box::new(SmbVerbindung { smb: self, s: self.anmelden()? }))
    }
}

/// Eigene Verbindung eines Workers.
struct SmbVerbindung<'a> {
    smb: &'a Smb,
    s: Sitzung,
}

impl SmbVerbindung<'_> {
    fn ort(&self, p: &RelPath) -> String {
        self.smb.ort(p)
    }

    /// Öffnet `p` zum Löschen bzw. Umbenennen.
    fn zum_aendern(&self, innen: &str, ort: &str) -> Result<OffenRes> {
        let args = FileCreateArgs::make_open_existing(FileAccessMask::new().with_delete(true).with_file_read_attributes(true));
        oeffnen(&self.s, innen, &args, ort).map(OffenRes)
    }

    fn loeschen(&self, p: &RelPath) -> Result<()> {
        let ort = self.ort(p);
        let r = match self.zum_aendern(&self.smb.innen(p), &ort) {
            Err(StellwerkError::NichtGefunden(_)) => return Ok(()),
            r => r?,
        };
        mit_ort(&ort, handle(&r.0).set_info(FileDispositionInformation { delete_pending: true.into() }))
    }
}

#[rustfmt::skip]
fn handle(r: &Resource) -> &ResourceHandle {
    match r { Resource::File(f) => f, Resource::Directory(d) => d, Resource::Pipe(p) => p }
}

impl ZielVerbindung for SmbVerbindung<'_> {
    fn ordner_anlegen(&mut self, p: &RelPath) -> Result<()> {
        let ort = self.ort(p);
        let args = FileCreateArgs::make_create_new(FileAttributes::new().with_directory(true), CreateOptions::new().with_directory_file(true));
        match oeffnen(&self.s, &self.smb.innen(p), &args, &ort) {
            Ok(r) => {
                drop(OffenRes(r));
                Ok(())
            }
            Err(StellwerkError::Vorhanden(m)) => {
                if self.ist_ordner(p)? {
                    Ok(())
                } else {
                    Err(StellwerkError::Vorhanden(m))
                }
            }
            Err(e) => Err(e),
        }
    }

    fn datei_schreiben(&mut self, p: &RelPath, q: QuellDatei, neu: bool, lauf_id: &str, abbruch: &AtomicBool) -> Result<Geschrieben> {
        let ort = self.ort(p);
        if neu && self.vorhanden(p)? {
            return Err(StellwerkError::Vorhanden(format!("{ort} – am Ziel inzwischen vorhanden, bitte neu vergleichen")));
        }
        let tmp = p.parent().expect("Datei liegt unter der Wurzel").join(&temp_name(p.name(), lauf_id)).expect("gültiger Name");
        let tmp_ort = self.ort(&tmp);
        let args = FileCreateArgs::make_create_new(FileAttributes::default(), CreateOptions::default());
        // Neu angelegt mit Datei-Optionen ist immer eine Datei
        let mut datei = Offen(oeffnen(&self.s, &self.smb.innen(&tmp), &args, &tmp_ort)?.unwrap_file());
        let QuellDatei { mut inhalt, geaendert, .. } = q;
        let ergebnis = (|| {
            let g = kopieren(&mut inhalt, &mut datei.0, abbruch, Path::new(p.as_str()), Path::new(&tmp_ort))?;
            datei.0.flush().map_err(verbindung(&tmp_ort))?;
            let mut info: FileBasicInformation = mit_ort(&tmp_ort, datei.0.query_info())?;
            info.last_write_time = filetime(geaendert);
            mit_ort(&tmp_ort, datei.0.set_info(info))?;
            let ziel = FileRenameInformation { replace_if_exists: true.into(), root_directory: 0, file_name: self.smb.innen(p).as_str().into() };
            mit_ort(&ort, datei.0.set_info(ziel))?;
            Ok(g)
        })();
        if ergebnis.is_err() {
            let _ = datei.0.set_info(FileDispositionInformation { delete_pending: true.into() });
        }
        ergebnis
    }

    fn rechte_setzen(&mut self, _p: &RelPath, _bits: u32) -> Result<()> {
        // SMB kennt keine Unix-Rechte – die Prüfung der Konfiguration weist darauf hin
        Ok(())
    }

    fn datei_loeschen(&mut self, p: &RelPath) -> Result<()> {
        self.loeschen(p)
    }

    fn ordner_loeschen(&mut self, p: &RelPath) -> Result<()> {
        self.loeschen(p)
    }

    fn umbenennen(&mut self, von: &RelPath, nach: &RelPath, ersetzen: bool) -> Result<()> {
        let ort = self.ort(von);
        let r = self.zum_aendern(&self.smb.innen(von), &ort)?;
        let info = FileRenameInformation { replace_if_exists: ersetzen.into(), root_directory: 0, file_name: self.smb.innen(nach).as_str().into() };
        mit_ort(&self.ort(nach), handle(&r.0).set_info(info))
    }

    fn ist_datei(&mut self, p: &RelPath) -> Result<bool> {
        Ok(self.art(p)? == Some(false))
    }
}

impl SmbVerbindung<'_> {
    /// `Some(true)`: Ordner, `Some(false)`: Datei, `None`: nichts da.
    fn art(&self, p: &RelPath) -> Result<Option<bool>> {
        let args = FileCreateArgs::make_open_existing(FileAccessMask::new().with_file_read_attributes(true));
        match oeffnen(&self.s, &self.smb.innen(p), &args, &self.ort(p)) {
            Err(StellwerkError::NichtGefunden(_)) => Ok(None),
            r => {
                let r = OffenRes(r?);
                Ok(Some(matches!(r.0, Resource::Directory(_))))
            }
        }
    }

    fn ist_ordner(&self, p: &RelPath) -> Result<bool> {
        Ok(self.art(p)? == Some(true))
    }

    fn vorhanden(&self, p: &RelPath) -> Result<bool> {
        Ok(self.art(p)?.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn e(pfad: &str) -> SmbEndpunkt {
        SmbEndpunkt { host: "pi".into(), port: 445, freigabe: "manitu".into(), pfad: pfad.into(), benutzer: "b".into(), konto_ref: "k".into() }
    }

    #[test]
    fn kennung_und_pfade() {
        assert_eq!(kennung(&e("/")), "smb://pi:445/manitu");
        assert_eq!(kennung(&e("/web/")), "smb://pi:445/manitu/web");
        let s = Smb::new(&e("/web/a"), "x");
        assert_eq!(s.innen(&RelPath::new("b/c.txt").unwrap()), r"web\a\b\c.txt");
        assert_eq!(Smb::new(&e("/"), "x").innen(&RelPath::wurzel()), "");
        assert_eq!(format!("{s:?}"), "Smb(smb://pi:445/manitu/web/a)");
    }

    fn info(attr: FileAttributes) -> FileDirectoryInformation {
        FileDirectoryInformation {
            file_index: 0,
            creation_time: FileTime::from(0u64),
            last_access_time: FileTime::from(0u64),
            last_write_time: filetime(SystemTime::UNIX_EPOCH + Duration::from_nanos(1_234_567_891)),
            change_time: FileTime::from(0u64),
            end_of_file: 7,
            allocation_size: 0,
            file_attributes: attr,
            file_name: "x".into(),
        }
    }

    #[test]
    fn eintraege_aus_dem_verzeichnis() {
        let w = RelPath::wurzel();
        let link = eintrag_aus(&w, "l", &info(FileAttributes::new().with_reparse_point(true))).unwrap();
        assert_eq!((link.art, link.groesse), (EintragArt::Symlink, 0));
        assert!(matches!(link.warnungen[..], [Warnung::Symlink { .. }]));
        let datei = eintrag_aus(&w, "a (conflicted copy).txt", &info(FileAttributes::new())).unwrap();
        assert_eq!((datei.art, datei.groesse, datei.mtime_ns), (EintragArt::Datei, 7, Some(1_234_567_800)));
        assert_eq!(datei.warnungen, vec![Warnung::Konfliktkopie]);
        let ordner = eintrag_aus(&w, "o", &info(FileAttributes::new().with_directory(true))).unwrap();
        assert_eq!((ordner.art, ordner.groesse), (EintragArt::Ordner, 0));
        let f = eintrag_aus(&w, "..", &info(FileAttributes::new())).unwrap_err();
        assert_eq!(f.pfad, "/..");
    }

    #[test]
    fn zeiten_vor_1970() {
        let t = SystemTime::UNIX_EPOCH - Duration::from_nanos(250);
        assert_eq!(zeit_ns(SystemTime::from(filetime(t))), Some(-300));
    }

    #[test]
    fn fehler_mit_ort() {
        let e = verbindung("ort")(std::io::Error::other("weg"));
        assert!(matches!(e, StellwerkError::Verbindung(m) if m == "ort: weg"));
        assert!(matches!(mit_ort::<()>("ort", Err(smb::Error::ConnectionStopped)), Err(StellwerkError::Verbindung(_))));
        // Eine Adresse, aus der kein UNC-Pfad wird
        let mut e = SmbEndpunkt { host: String::new(), ..super::tests::e("/") };
        e.freigabe = String::new();
        assert!(Smb::new(&e, "x").anmelden().is_err());
    }

    #[test]
    fn fehler_werden_uebersetzt() {
        let f = |s: Status| fehler("ort", smb::Error::UnexpectedMessageStatus(s as u32));
        assert!(matches!(f(Status::LogonFailure), StellwerkError::Anmeldung(_)));
        assert!(matches!(f(Status::ObjectNameNotFound), StellwerkError::NichtGefunden(_)));
        assert!(matches!(f(Status::ObjectPathNotFound), StellwerkError::NichtGefunden(_)));
        assert!(matches!(f(Status::BadNetworkName), StellwerkError::NichtGefunden(m) if m.starts_with("Freigabe")));
        assert!(matches!(f(Status::AccessDenied), StellwerkError::KeineRechte(_)));
        assert!(matches!(f(Status::ObjectNameCollision), StellwerkError::Vorhanden(_)));
        assert!(matches!(f(Status::DirectoryNotEmpty), StellwerkError::Verbindung(_)));
        assert!(matches!(fehler("ort", smb::Error::ConnectionStopped), StellwerkError::Verbindung(_)));
    }
}
