//! SFTP-Server als Endpunkt (v0.7.2) – über libssh2 (`ssh2`), synchron wie der Rest
//! des Kerns.
//!
//! - Verbunden wird nur, wenn der Host-Schlüssel dem bewusst bestätigten entspricht
//!   (`hostschluessel`, `SHA256:…` wie bei OpenSSH). Sonst [`StellwerkError::HostSchluessel`]
//!   mit dem gesehenen Fingerabdruck – zum Prüfen und Bestätigen.
//! - Anmeldung mit Benutzer und Passwort (aus dem Schlüsselbund).
//! - Verglichen wird wie bei FTPS über den letzten bekannten Stand; Zeiten kennt SFTP
//!   sekundengenau.
//! - Als Ziel: Temporärdatei, Zeit der Quelle setzen, umbenennen. Ersetzt der Server beim
//!   Umbenennen nicht (SFTP-Version 3, z. B. OpenSSH), wird die alte Datei gelöscht und
//!   sofort umbenannt.
//! - Jeder Worker erhält eine eigene Verbindung.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ssh2::{ErrorCode, FileStat, HashType, Session};

use super::{kopieren, temp_name, Geschrieben, QuellDatei, QuellEndpunkt, ZielEndpunkt, ZielVerbindung};
use crate::error::{Result, StellwerkError};
use crate::model::{Eintrag, EintragArt, RelPath, SftpEndpunkt, Warnung};
use crate::rules::RegelWerk;
use crate::model::RegelTyp;
use crate::scan::{aus_liste, ist_konfliktkopie, ScanErgebnis, ScanFehler};

/// Gleichzeitige Verbindungen beim Schreiben – viele Hoster begrenzen SSH-Sitzungen.
pub const SFTP_WORKER: usize = 2;

/// Wartezeit auf den Server, bevor eine Operation als gescheitert gilt.
pub const ZEITLIMIT: Duration = Duration::from_secs(30);

// Fehlercodes von SFTP (draft-ietf-secsh-filexfer) und libssh2
const SFTP_FEHLT: i32 = 2;
const SFTP_VERWEIGERT: i32 = 3;
const SFTP_PFAD_FEHLT: i32 = 10;
const SSH_ANMELDUNG: i32 = -18;
/// libssh2: Schlüsseldatei nicht lesbar (falsche Passphrase, unbekanntes Format).
const SSH_DATEI: i32 = -16;
/// libssh2: Server hat den Schlüssel nicht anerkannt.
const SSH_UNBESTAETIGT: i32 = -19;

/// Ergebnis von „Verbindung testen“.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SftpVerbindungstest {
    /// Kennung der Server-Software (`SSH-2.0-OpenSSH_9.6`).
    pub server: Option<String>,
    /// Ordner der Stufe, wie der Server ihn auflöst.
    pub verzeichnis: String,
    /// Einträge im Ordner (ohne Unterordner).
    pub eintraege: usize,
    /// Fingerabdruck des Host-Schlüssels (`SHA256:…`).
    pub hostschluessel: String,
}

/// Ein SFTP-Server mit Zugangsdaten.
pub struct Sftp {
    ziel: SftpEndpunkt,
    passwort: String,
    zeitlimit: Duration,
}

impl std::fmt::Debug for Sftp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Ohne Passwort
        write!(f, "Sftp({})", kennung(&self.ziel))
    }
}

/// `sftp://host:port/pfad` – Kennung für Vergleich und Plan.
pub fn kennung(e: &SftpEndpunkt) -> String {
    format!("sftp://{}:{}{}", e.host, e.port, e.pfad)
}

/// Eine angemeldete Verbindung.
struct Sitzung {
    sess: Session,
    sftp: ssh2::Sftp,
}

impl Sitzung {
    fn ist_ordner(&self, abs: &str) -> bool {
        self.sftp.stat(Path::new(abs)).is_ok_and(|s| s.is_dir())
    }

    fn vorhanden(&self, abs: &str) -> bool {
        self.sftp.lstat(Path::new(abs)).is_ok()
    }
}

/// Fingerabdruck wie bei OpenSSH: `SHA256:` + Base64 ohne `=`.
pub fn fingerprint_text(hash: &[u8]) -> String {
    const ZEICHEN: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::from("SHA256:");
    for block in hash.chunks(3) {
        let n = block.iter().enumerate().fold(0u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
        for i in 0..=block.len() {
            s.push(ZEICHEN[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    s
}

/// Fehler der Bibliothek mit Ort übersetzen (Meldungen ohne Passwort).
fn fehler(ort: &str, e: &ssh2::Error) -> StellwerkError {
    match e.code() {
        ErrorCode::SFTP(SFTP_FEHLT | SFTP_PFAD_FEHLT) => StellwerkError::NichtGefunden(ort.to_string()),
        ErrorCode::SFTP(SFTP_VERWEIGERT) => StellwerkError::KeineRechte(format!("{ort}: Zugriff verweigert")),
        ErrorCode::Session(SSH_ANMELDUNG) => StellwerkError::Anmeldung(format!("{ort}: Benutzer oder Passwort falsch")),
        _ => StellwerkError::Verbindung(format!("{ort}: {}", e.message())),
    }
}

/// Fehler der Anmeldung per SSH-Schlüssel (v0.7.2b) in verständlichen Worten.
fn schluessel_fehler(ort: &str, e: &ssh2::Error) -> StellwerkError {
    match e.code() {
        ErrorCode::Session(SSH_DATEI) => {
            StellwerkError::Anmeldung(format!("{ort}: SSH-Schlüssel nicht lesbar – Passphrase falsch oder Format unbekannt"))
        }
        ErrorCode::Session(SSH_ANMELDUNG | SSH_UNBESTAETIGT) => {
            StellwerkError::Anmeldung(format!("{ort}: Der Server kennt diesen SSH-Schlüssel nicht (oder den Benutzer)"))
        }
        _ => fehler(ort, e),
    }
}

/// Pfad des privaten Schlüssels; `~/` steht für den Benutzerordner.
fn schluessel_pfad(p: &str) -> PathBuf {
    match (p.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => PathBuf::from(home).join(rest),
        _ => PathBuf::from(p),
    }
}

fn mit_ort<T>(ort: &str, r: std::result::Result<T, ssh2::Error>) -> Result<T> {
    r.map_err(|e| fehler(ort, &e))
}

/// Ein-/Ausgabefehler beim Übertragen als Verbindungsfehler mit Ort.
fn verbindung(ort: &str) -> impl Fn(std::io::Error) -> StellwerkError + '_ {
    move |e| StellwerkError::Verbindung(format!("{ort}: {e}"))
}

/// Sekunden seit 1970 (Zeitpunkte davor als 0 – SFTP kennt keine negativen Zeiten).
fn sekunden(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

impl Sftp {
    pub fn new(ziel: &SftpEndpunkt, passwort: &str) -> Self {
        Self { ziel: ziel.clone(), passwort: passwort.to_string(), zeitlimit: ZEITLIMIT }
    }

    /// Kürzeres Zeitlimit (Tests).
    pub fn zeitlimit(mut self, t: Duration) -> Self {
        self.zeitlimit = t;
        self
    }

    fn ort(&self) -> String {
        format!("{}:{}", self.ziel.host, self.ziel.port)
    }

    /// Absoluter Serverpfad zu einem Pfad relativ zum Ordner der Stufe.
    fn absolut(&self, p: &RelPath) -> String {
        match p.as_str() {
            "" => self.ziel.pfad.clone(),
            p => format!("{}/{p}", self.ziel.pfad.trim_end_matches('/')),
        }
    }

    /// Verbindet, prüft den Host-Schlüssel, meldet an und prüft den Ordner der Stufe.
    fn anmelden(&self) -> Result<Sitzung> {
        let ort = self.ort();
        let adressen: Vec<SocketAddr> = (self.ziel.host.as_str(), self.ziel.port)
            .to_socket_addrs()
            .map_err(|e| StellwerkError::Verbindung(format!("{ort}: Name nicht auflösbar ({e})")))?
            .collect();
        let tcp = adressen
            .iter()
            .find_map(|a| TcpStream::connect_timeout(a, self.zeitlimit).ok())
            .ok_or_else(|| StellwerkError::Verbindung(format!("{ort} nicht erreichbar")))?;
        let mut sess = mit_ort(&ort, Session::new())?;
        sess.set_tcp_stream(tcp);
        sess.set_timeout(u32::try_from(self.zeitlimit.as_millis()).unwrap_or(u32::MAX));
        mit_ort(&ort, sess.handshake())?;
        self.schluessel_pruefen(&sess)?;
        self.anmeldung(&sess, &ort)?;
        let sftp = mit_ort(&ort, sess.sftp())?;
        let s = Sitzung { sess, sftp };
        if !s.ist_ordner(&self.ziel.pfad) {
            return Err(StellwerkError::NichtGefunden(format!("Serverpfad {} auf {ort}", self.ziel.pfad)));
        }
        Ok(s)
    }

    /// Mit Passwort oder – wenn eingetragen – mit SSH-Schlüssel (v0.7.2b); dann ist
    /// `passwort` dessen Passphrase (leer: ohne).
    fn anmeldung(&self, sess: &Session, ort: &str) -> Result<()> {
        let Some(datei) = self.ziel.schluessel.as_deref().map(schluessel_pfad) else {
            return mit_ort(ort, sess.userauth_password(&self.ziel.benutzer, &self.passwort));
        };
        if !datei.is_file() {
            return Err(StellwerkError::NichtGefunden(format!("SSH-Schlüssel {}", datei.display())));
        }
        let passphrase = Some(self.passwort.as_str()).filter(|p| !p.is_empty());
        sess.userauth_pubkey_file(&self.ziel.benutzer, None, &datei, passphrase).map_err(|e| schluessel_fehler(ort, &e))
    }

    /// Nur mit dem bestätigten Host-Schlüssel weiter.
    fn schluessel_pruefen(&self, sess: &Session) -> Result<()> {
        let gesehen = fingerprint_text(sess.host_key_hash(HashType::Sha256).unwrap_or_default());
        let meldung = match self.ziel.hostschluessel.as_deref() {
            Some(b) if b == gesehen => return Ok(()),
            Some(_) => format!(
                "Der Host-Schlüssel von {} hat sich geändert – nur bestätigen, wenn der Betreiber das angekündigt hat",
                self.ort()
            ),
            None => format!("Host-Schlüssel von {} noch nicht bestätigt", self.ort()),
        };
        Err(StellwerkError::HostSchluessel { meldung, fingerprint: gesehen })
    }

    /// Liest den Ordner der Stufe rekursiv ein. Ignorierte Ordner werden nicht
    /// durchlaufen; nicht lesbare Unterordner werden gemeldet, nicht abgebrochen.
    pub fn auflisten(&self, regeln: &RegelWerk, abbruch: &AtomicBool, fortschritt: &mut dyn FnMut(usize)) -> Result<ScanErgebnis> {
        let s = self.anmelden()?;
        let mut liste = Vec::new();
        let mut fehler_liste = Vec::new();
        let mut offen = vec![RelPath::wurzel()];
        while let Some(ordner) = offen.pop() {
            if abbruch.load(Ordering::Relaxed) {
                return Err(StellwerkError::Abbruch);
            }
            let abs = self.absolut(&ordner);
            let roh = match s.sftp.readdir(Path::new(&abs)) {
                Ok(r) => r,
                Err(e) if ordner.ist_wurzel() => return Err(fehler(&abs, &e)),
                Err(e) => {
                    fehler_liste.push(ScanFehler { pfad: ordner.to_string(), meldung: format!("Ordner nicht lesbar: {}", fehler(&abs, &e)) });
                    continue;
                }
            };
            let eintraege: Vec<_> = roh.iter().map(|(p, st)| eintrag_aus(&ordner, p, st)).collect();
            let (gut, schlecht): (Vec<_>, Vec<_>) = eintraege.into_iter().partition(std::result::Result::is_ok);
            fehler_liste.extend(schlecht.into_iter().filter_map(std::result::Result::err));
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
        erg.fehler.extend(fehler_liste);
        Ok(erg)
    }

    /// „Inhalte prüfen“: liest die Dateien einmal und bildet ihren BLAKE3-Hash. Fehler
    /// gelten je Datei; ein Abbruch beendet alles.
    pub fn inhalte_hashen(
        &self,
        pfade: &[RelPath],
        abbruch: &AtomicBool,
        fortschritt: &mut dyn FnMut(usize),
    ) -> Result<Vec<(RelPath, std::result::Result<Geschrieben, String>)>> {
        let s = self.anmelden()?;
        let mut erg = Vec::new();
        for (i, p) in pfade.iter().enumerate() {
            let abs = self.absolut(p);
            match lesen_hash(&s, &abs, abbruch) {
                Err(StellwerkError::Abbruch) => return Err(StellwerkError::Abbruch),
                r => erg.push((p.clone(), r.map_err(|e| e.to_string()))),
            }
            fortschritt(i + 1);
        }
        Ok(erg)
    }

    /// „Verbindung testen“: Host-Schlüssel, Anmeldung, Ordner. Liest nur.
    pub fn verbindung_testen(&self) -> Result<SftpVerbindungstest> {
        let s = self.anmelden()?;
        let abs = &self.ziel.pfad;
        let verzeichnis = mit_ort(abs, s.sftp.realpath(Path::new(abs)))?.display().to_string();
        let eintraege = mit_ort(abs, s.sftp.readdir(Path::new(abs)))?.len();
        Ok(SftpVerbindungstest {
            server: s.sess.banner().map(str::to_string),
            verzeichnis,
            eintraege,
            hostschluessel: fingerprint_text(s.sess.host_key_hash(HashType::Sha256).unwrap_or_default()),
        })
    }

    /// Diesen Server als Quelle lesen (v0.6.5 wie FTPS). Verbindungen werden zwischen
    /// Dateien wiederverwendet (je Worker eine).
    pub fn als_quelle(&self) -> SftpQuelle<'_> {
        SftpQuelle { sftp: self, frei: Arc::default() }
    }
}

/// Liest eine Datei und bildet ihren Hash.
fn lesen_hash(s: &Sitzung, abs: &str, abbruch: &AtomicBool) -> Result<Geschrieben> {
    let mut datei = mit_ort(abs, s.sftp.open(Path::new(abs)))?;
    kopieren(&mut datei, &mut std::io::sink(), abbruch, Path::new(abs), Path::new("(Hash)"))
}

/// Einen Verzeichniseintrag in einen [`Eintrag`] übersetzen.
fn eintrag_aus(ordner: &RelPath, pfad: &Path, st: &FileStat) -> std::result::Result<Eintrag, ScanFehler> {
    let name = pfad.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    let ungueltig = || ScanFehler { pfad: format!("{}/{}", ordner.as_str(), pfad.display()), meldung: "Name ist als Pfad ungültig".into() };
    let pfad = ordner.join(name).ok().filter(|_| !name.is_empty()).ok_or_else(ungueltig)?;
    let art = match st.file_type() {
        t if t.is_symlink() => EintragArt::Symlink,
        t if t.is_dir() => EintragArt::Ordner,
        _ => EintragArt::Datei,
    };
    let warnungen = [
        (art == EintragArt::Symlink).then(|| Warnung::Symlink { ziel: String::new() }),
        ist_konfliktkopie(name).then_some(Warnung::Konfliktkopie),
    ]
    .into_iter()
    .flatten()
    .collect();
    Ok(Eintrag {
        groesse: if art == EintragArt::Datei { st.size.unwrap_or(0) } else { 0 },
        mtime_ns: st.mtime.and_then(|s| i64::try_from(s).ok()).map(|s| s * 1_000_000_000),
        pfad,
        art,
        nur_online: false,
        hash: None,
        warnungen,
    })
}

/// Ein SFTP-Server als Quelle – nur lesend (E3). Die Änderungszeit kommt vom Server.
pub struct SftpQuelle<'a> {
    sftp: &'a Sftp,
    frei: Arc<Mutex<Vec<Sitzung>>>,
}

/// Eine geöffnete Datei; gibt ihre Verbindung am Ende zurück in den Vorrat.
struct Leser {
    datei: Option<ssh2::File>,
    sitzung: Option<Sitzung>,
    frei: Arc<Mutex<Vec<Sitzung>>>,
}

impl Read for Leser {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.datei.as_mut().expect("offen").read(buf)
    }
}

impl Drop for Leser {
    fn drop(&mut self) {
        // Erst die Datei schließen, dann die Verbindung freigeben
        drop(self.datei.take());
        let s = self.sitzung.take().expect("Sitzung");
        self.frei.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(s);
    }
}

impl QuellEndpunkt for SftpQuelle<'_> {
    fn oeffnen(&self, p: &RelPath) -> Result<QuellDatei> {
        let abs = self.sftp.absolut(p);
        let frei = self.frei.lock().unwrap_or_else(std::sync::PoisonError::into_inner).pop();
        let s = match frei {
            Some(s) => s,
            None => self.sftp.anmelden()?,
        };
        let mut datei = mit_ort(&abs, s.sftp.open(Path::new(&abs)))?;
        let st = mit_ort(&abs, datei.stat())?;
        if !st.is_file() {
            return Err(StellwerkError::NichtUnterstuetzt(format!("{abs} ist keine Datei")));
        }
        let geaendert = UNIX_EPOCH + Duration::from_secs(st.mtime.unwrap_or(0));
        let leser = Leser { datei: Some(datei), sitzung: Some(s), frei: self.frei.clone() };
        Ok(QuellDatei { inhalt: Box::new(leser), groesse: st.size.unwrap_or(0), geaendert, modus: None })
    }
}

impl ZielEndpunkt for Sftp {
    fn verbinden(&self) -> Result<Box<dyn ZielVerbindung + '_>> {
        Ok(Box::new(SftpVerbindung { ziel: self, s: self.anmelden()? }))
    }
}

/// Eigene Verbindung eines Workers.
struct SftpVerbindung<'a> {
    ziel: &'a Sftp,
    s: Sitzung,
}

impl SftpVerbindung<'_> {
    /// Hochladen nach `tmp`, Zeit setzen, umbenennen – ohne Aufräumen (macht der Aufrufer).
    fn hochladen(&mut self, q: QuellDatei, tmp: &str, ziel: &str, neu: bool, ort: &RelPath, abbruch: &AtomicBool) -> Result<Geschrieben> {
        let QuellDatei { mut inhalt, geaendert, .. } = q;
        let mut datei = mit_ort(tmp, self.s.sftp.create(Path::new(tmp)))?;
        let g = kopieren(&mut inhalt, &mut datei, abbruch, Path::new(ort.as_str()), Path::new(tmp))?;
        datei.flush().map_err(verbindung(tmp))?;
        drop(datei);
        let t = sekunden(geaendert);
        let zeit = FileStat { size: None, uid: None, gid: None, perm: None, atime: Some(t), mtime: Some(t) };
        mit_ort(tmp, self.s.sftp.setstat(Path::new(tmp), zeit))?;
        self.umbenennen_absolut(tmp, ziel, neu)?;
        Ok(g)
    }

    /// Umbenennen. Beim Ersetzen: Überschreibt der Server nicht (SFTP 3), wird die alte
    /// Datei gelöscht und sofort umbenannt.
    fn umbenennen_absolut(&mut self, von: &str, nach: &str, neu: bool) -> Result<()> {
        match self.s.sftp.rename(Path::new(von), Path::new(nach), None) {
            Ok(()) => Ok(()),
            Err(_) if !neu && self.s.vorhanden(nach) => {
                mit_ort(nach, self.s.sftp.unlink(Path::new(nach)))?;
                mit_ort(nach, self.s.sftp.rename(Path::new(von), Path::new(nach), None))
            }
            Err(e) => Err(fehler(nach, &e)),
        }
    }
}

impl ZielVerbindung for SftpVerbindung<'_> {
    fn ordner_anlegen(&mut self, p: &RelPath) -> Result<()> {
        let abs = self.ziel.absolut(p);
        match self.s.sftp.mkdir(Path::new(&abs), 0o755) {
            Err(_) if self.s.ist_ordner(&abs) => Ok(()),
            r => mit_ort(&abs, r),
        }
    }

    fn datei_schreiben(&mut self, p: &RelPath, q: QuellDatei, neu: bool, lauf_id: &str, abbruch: &AtomicBool) -> Result<Geschrieben> {
        let ziel = self.ziel.absolut(p);
        if neu && self.s.vorhanden(&ziel) {
            return Err(StellwerkError::Vorhanden(format!("{ziel} – am Server inzwischen vorhanden, bitte neu vergleichen")));
        }
        let (ordner, name) = ziel.rsplit_once('/').expect("absoluter Pfad");
        let tmp = format!("{ordner}/{}", temp_name(name, lauf_id));
        let ergebnis = self.hochladen(q, &tmp, &ziel, neu, p, abbruch);
        if ergebnis.is_err() {
            let _ = self.s.sftp.unlink(Path::new(&tmp));
        }
        ergebnis
    }

    fn rechte_setzen(&mut self, p: &RelPath, bits: u32) -> Result<()> {
        let abs = self.ziel.absolut(p);
        let st = FileStat { size: None, uid: None, gid: None, perm: Some(bits), atime: None, mtime: None };
        mit_ort(&abs, self.s.sftp.setstat(Path::new(&abs), st))
    }

    fn datei_loeschen(&mut self, p: &RelPath) -> Result<()> {
        let abs = self.ziel.absolut(p);
        match self.s.sftp.unlink(Path::new(&abs)) {
            Ok(()) => Ok(()),
            Err(_) if self.s.ist_ordner(&abs) => {
                Err(StellwerkError::NichtUnterstuetzt(format!("{abs} ist inzwischen ein Ordner – bitte neu vergleichen")))
            }
            Err(_) if !self.s.vorhanden(&abs) => Ok(()),
            Err(e) => Err(fehler(&abs, &e)),
        }
    }

    fn ordner_loeschen(&mut self, p: &RelPath) -> Result<()> {
        let abs = self.ziel.absolut(p);
        match self.s.sftp.rmdir(Path::new(&abs)) {
            Err(_) if !self.s.ist_ordner(&abs) => Ok(()),
            r => mit_ort(&abs, r),
        }
    }

    fn umbenennen(&mut self, von: &RelPath, nach: &RelPath, ersetzen: bool) -> Result<()> {
        let (v, n) = (self.ziel.absolut(von), self.ziel.absolut(nach));
        if !ersetzen && self.s.vorhanden(&n) {
            return Err(StellwerkError::Vorhanden(format!("{n} – am Server inzwischen vorhanden, bitte neu vergleichen")));
        }
        self.umbenennen_absolut(&v, &n, !ersetzen)
    }

    fn ist_datei(&mut self, p: &RelPath) -> Result<bool> {
        let abs = self.ziel.absolut(p);
        Ok(self.s.sftp.stat(Path::new(&abs)).is_ok_and(|s| s.is_file()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schluessel_fehler_werden_uebersetzt() {
        let f = |c| schluessel_fehler("ort", &ssh2::Error::new(c, "x")).to_string();
        assert!(f(ErrorCode::Session(SSH_DATEI)).contains("Passphrase falsch"));
        assert!(f(ErrorCode::Session(SSH_ANMELDUNG)).contains("kennt diesen SSH-Schlüssel nicht"));
        assert!(f(ErrorCode::Session(SSH_UNBESTAETIGT)).contains("kennt diesen SSH-Schlüssel nicht"));
        assert!(f(ErrorCode::Session(-7)).contains("ort: x"));
    }

    #[test]
    fn schluessel_pfad_mit_benutzerordner() {
        let home = std::env::var("HOME").unwrap();
        assert_eq!(schluessel_pfad("~/.ssh/stellwerk"), PathBuf::from(home).join(".ssh/stellwerk"));
        assert_eq!(schluessel_pfad("/a/b"), PathBuf::from("/a/b"));
        assert_eq!(schluessel_pfad("~x"), PathBuf::from("~x"));
    }

    #[test]
    fn fingerprint_wie_openssh() {
        // ssh-keygen -lf zeigt für diesen Hash dieselbe Zeichenfolge
        let hash: Vec<u8> = (0u8..32).collect();
        assert_eq!(fingerprint_text(&hash), "SHA256:AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8");
        assert_eq!(fingerprint_text(&[0xff]), "SHA256:/w");
        assert_eq!(fingerprint_text(&[0xff, 0xfe]), "SHA256://4");
    }

    #[test]
    fn fehler_werden_uebersetzt() {
        let f = |c| fehler("ort", &ssh2::Error::new(c, "x"));
        assert!(matches!(f(ErrorCode::SFTP(SFTP_FEHLT)), StellwerkError::NichtGefunden(_)));
        assert!(matches!(f(ErrorCode::SFTP(SFTP_PFAD_FEHLT)), StellwerkError::NichtGefunden(_)));
        assert!(matches!(f(ErrorCode::SFTP(SFTP_VERWEIGERT)), StellwerkError::KeineRechte(_)));
        assert!(matches!(f(ErrorCode::Session(SSH_ANMELDUNG)), StellwerkError::Anmeldung(_)));
        assert!(matches!(f(ErrorCode::SFTP(4)), StellwerkError::Verbindung(m) if m == "ort: x"));
        let e = verbindung("ort")(std::io::Error::other("weg"));
        assert!(matches!(e, StellwerkError::Verbindung(m) if m == "ort: weg"));
    }

    #[test]
    fn zeiten_und_kennung() {
        assert_eq!(sekunden(UNIX_EPOCH + Duration::from_millis(2_500)), 2);
        assert_eq!(sekunden(UNIX_EPOCH - Duration::from_secs(1)), 0);
        let e = SftpEndpunkt { host: "h".into(), port: 23, pfad: "/web/".into(), benutzer: "b".into(), konto_ref: "k".into(), hostschluessel: None, schluessel: None };
        assert_eq!(kennung(&e), "sftp://h:23/web/");
        let s = Sftp::new(&e, "geheim");
        assert_eq!(format!("{s:?}"), "Sftp(sftp://h:23/web/)");
        assert_eq!(s.absolut(&RelPath::wurzel()), "/web/");
        assert_eq!(s.absolut(&RelPath::new("a/b").unwrap()), "/web/a/b");
    }

    #[test]
    fn eintraege_aus_dem_verzeichnis() {
        let st = |perm: u32| FileStat { size: Some(7), uid: None, gid: None, perm: Some(perm), atime: None, mtime: Some(1_700_000_000) };
        let w = RelPath::wurzel();
        let d = eintrag_aus(&w, Path::new("/x/a (conflicted copy).txt"), &st(0o100644)).unwrap();
        assert_eq!((d.art, d.groesse, d.mtime_ns), (EintragArt::Datei, 7, Some(1_700_000_000_000_000_000)));
        assert_eq!(d.warnungen, vec![Warnung::Konfliktkopie]);
        let o = eintrag_aus(&w, Path::new("o"), &st(0o040755)).unwrap();
        assert_eq!((o.art, o.groesse), (EintragArt::Ordner, 0));
        let l = eintrag_aus(&w, Path::new("l"), &st(0o120777)).unwrap();
        assert!(matches!(l.warnungen[..], [Warnung::Symlink { .. }]));
        assert_eq!(eintrag_aus(&w, Path::new(".."), &st(0o100644)).unwrap_err().pfad, "/..");
        assert!(eintrag_aus(&w, Path::new("/"), &st(0o100644)).is_err());
    }
}
