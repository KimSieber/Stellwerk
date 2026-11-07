//! FTPS-Endpunkt (KONZEPT E6, E7, Kap. 8; ARCHITEKTUR Kap. 5, 11).
//!
//! - **Explizites TLS ist Pflicht** (`AUTH TLS`, dann `PBSZ 0`/`PROT P` – auch der
//!   Datenkanal ist verschlüsselt). Ein Server ohne TLS wird abgewiesen, Klartext-FTP
//!   gibt es nicht.
//! - **Zertifikat:** geprüft vom Betriebssystem (macOS-Schlüsselbund/Vertrauenseinstellungen).
//!   Erkennt es das Zertifikat nicht an, gilt es nur, wenn sein SHA-256-Fingerprint
//!   in der Stufe bewusst bestätigt wurde (`zertifikat`). Sonst meldet der Fehler
//!   den Fingerprint, damit er geprüft und bestätigt werden kann.
//! - **Immer binär** (`TYPE I`), Zeilenenden werden nie verändert.
//! - **Zeitlimits** für Verbindungsaufbau, Steuer- und Datenkanal – ein hängender
//!   Server blockiert keinen Worker auf Dauer.
//! - Passiver Modus mit NAT-Umgehung: Meldet der Server im `PASV` eine private
//!   Adresse, wird die Adresse der Steuerverbindung verwendet.
//! - Das Passwort kommt vom Aufrufer (Schlüsselbund) und wird nirgends ausgegeben.

use std::io;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
use rustls_platform_verifier::Verifier;
use serde::Serialize;
use sha2::{Digest, Sha256};
use suppaftp::types::FileType;
use suppaftp::{FtpError, RustlsConnector, RustlsFtpStream, Status};

use crate::error::{Result, StellwerkError};
use crate::model::FtpsEndpunkt;

/// Zeitlimit für Verbindungsaufbau und jede Antwort des Servers.
pub const ZEITLIMIT: Duration = Duration::from_secs(30);

/// SHA-256 als `AB:CD:…` (Großbuchstaben, Doppelpunkte).
pub fn fingerprint_text(f: &[u8; 32]) -> String {
    f.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":")
}

/// Liest einen Fingerprint – Groß/Klein, Doppelpunkte und Leerzeichen sind egal.
/// `None`, wenn es kein SHA-256-Wert ist.
pub fn fingerprint_bytes(text: &str) -> Option<[u8; 32]> {
    let hex: Vec<u8> = text.bytes().filter(|c| *c != b':' && !c.is_ascii_whitespace()).collect();
    if hex.len() != 64 {
        return None;
    }
    let mut f = [0u8; 32];
    for (i, paar) in hex.chunks(2).enumerate() {
        f[i] = u8::from_str_radix(std::str::from_utf8(paar).ok()?, 16).ok()?;
    }
    Some(f)
}

/// Was die Zertifikatsprüfung zuletzt gesehen hat.
#[derive(Debug, Clone, Default)]
struct Gesehen {
    fingerprint: String,
    /// Vom Betriebssystem anerkannt (sonst per bestätigtem Fingerprint oder abgelehnt).
    system: bool,
    /// Ablehnungsgrund, wenn das Zertifikat nicht galt.
    abgelehnt: Option<String>,
}

/// Prüft zuerst wie das Betriebssystem; lehnt es ab, zählt nur der bestätigte Fingerprint.
#[derive(Debug)]
struct Pruefer {
    system: Verifier,
    bestaetigt: Option<[u8; 32]>,
    gesehen: Arc<Mutex<Gesehen>>,
}

impl ServerCertVerifier for Pruefer {
    fn verify_server_cert(
        &self,
        zertifikat: &CertificateDer<'_>,
        zwischen: &[CertificateDer<'_>],
        name: &ServerName<'_>,
        ocsp: &[u8],
        jetzt: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        let f: [u8; 32] = Sha256::digest(zertifikat).into();
        let system = self.system.verify_server_cert(zertifikat, zwischen, name, ocsp, jetzt);
        let mut g = self.gesehen.lock().expect("Zertifikatsstand");
        *g = Gesehen { fingerprint: fingerprint_text(&f), system: system.is_ok(), abgelehnt: None };
        match system {
            Err(_) if self.bestaetigt == Some(f) => Ok(ServerCertVerified::assertion()),
            Err(e) => {
                g.abgelehnt = Some(e.to_string());
                Err(e)
            }
            ok => ok,
        }
    }

    fn verify_tls12_signature(
        &self,
        nachricht: &[u8],
        zertifikat: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        self.system.verify_tls12_signature(nachricht, zertifikat, dss)
    }

    fn verify_tls13_signature(
        &self,
        nachricht: &[u8],
        zertifikat: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        self.system.verify_tls13_signature(nachricht, zertifikat, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.system.supported_verify_schemes()
    }
}

/// Fähigkeiten des Servers (aus `FEAT` und `SITE HELP`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Faehigkeiten {
    /// `MLSD`: maschinenlesbare Verzeichnisliste (sonst `LIST`).
    pub mlsd: bool,
    /// `MFMT`: Änderungszeit setzen (sonst nur über das Manifest).
    pub mfmt: bool,
    /// `SITE CHMOD`: ja / nein / unbekannt (Server nennt seine SITE-Befehle nicht).
    pub chmod: Option<bool>,
    /// `REST STREAM`: abgebrochene Übertragungen fortsetzen.
    pub fortsetzen: bool,
    pub utf8: bool,
}

/// Zertifikat der Verbindung.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Zertifikatsstand {
    pub fingerprint: String,
    /// `true`: vom Betriebssystem anerkannt; `false`: über den bestätigten Fingerprint.
    pub vom_system: bool,
}

/// Ergebnis von „Verbindung testen“.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Verbindungstest {
    pub begruessung: Option<String>,
    /// Serverpfad der Stufe, wie der Server ihn meldet (`PWD`).
    pub verzeichnis: String,
    /// Einträge im Serverpfad – zeigt, dass auch der Datenkanal funktioniert.
    pub eintraege: usize,
    pub faehigkeiten: Faehigkeiten,
    pub zertifikat: Zertifikatsstand,
}

/// Ein FTPS-Ziel mit Zugangsdaten.
pub struct Ftps {
    ziel: FtpsEndpunkt,
    passwort: String,
    tls: Arc<ClientConfig>,
    gesehen: Arc<Mutex<Gesehen>>,
    zeitlimit: Duration,
}

fn tls_einrichtung(e: rustls::Error) -> StellwerkError {
    StellwerkError::Verbindung(format!("TLS kann nicht eingerichtet werden: {e}"))
}

impl Ftps {
    /// Ziel mit der Zertifikatsprüfung des Betriebssystems.
    pub fn new(ziel: &FtpsEndpunkt, passwort: &str) -> Result<Self> {
        Self::mit_wurzeln(ziel, passwort, Vec::new())
    }

    /// Wie [`Ftps::new`], zusätzlich mit eigenen Wurzelzertifikaten (DER) – für Tests
    /// mit einer eigenen Zertifizierungsstelle.
    pub fn mit_wurzeln(ziel: &FtpsEndpunkt, passwort: &str, wurzeln: Vec<Vec<u8>>) -> Result<Self> {
        let anbieter = Arc::new(rustls::crypto::ring::default_provider());
        let system = if wurzeln.is_empty() {
            Verifier::new(anbieter.clone())
        } else {
            Verifier::new_with_extra_roots(wurzeln.into_iter().map(CertificateDer::from), anbieter.clone())
        }
        .map_err(tls_einrichtung)?;
        let gesehen = Arc::new(Mutex::new(Gesehen::default()));
        let pruefer = Pruefer {
            system,
            bestaetigt: ziel.zertifikat.as_deref().and_then(fingerprint_bytes),
            gesehen: gesehen.clone(),
        };
        let versionen = ClientConfig::builder_with_provider(anbieter).with_safe_default_protocol_versions();
        let tls = versionen
            .map_err(tls_einrichtung)?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(pruefer))
            .with_no_client_auth();
        Ok(Self { ziel: ziel.clone(), passwort: passwort.to_string(), tls: Arc::new(tls), gesehen, zeitlimit: ZEITLIMIT })
    }

    /// Anderes Zeitlimit (Tests).
    pub fn zeitlimit(mut self, t: Duration) -> Self {
        self.zeitlimit = t;
        self
    }

    fn ort(&self) -> String {
        format!("{}:{}", self.ziel.host, self.ziel.port)
    }

    /// Verbindet, sichert mit TLS, meldet an, schaltet auf binär und wechselt in den Serverpfad.
    pub(crate) fn anmelden(&self) -> Result<RustlsFtpStream> {
        let ort = self.ort();
        let adressen: Vec<SocketAddr> = (self.ziel.host.as_str(), self.ziel.port)
            .to_socket_addrs()
            .map_err(|e| StellwerkError::Verbindung(format!("{ort}: Name nicht auflösbar ({e})")))?
            .collect();
        let mut fehler = io::Error::new(io::ErrorKind::NotFound, "keine Adresse");
        for a in adressen {
            match TcpStream::connect_timeout(&a, self.zeitlimit) {
                Ok(s) => return self.sichern(s),
                Err(e) => fehler = e,
            }
        }
        Err(StellwerkError::Verbindung(format!("{ort} nicht erreichbar: {fehler}")))
    }

    fn sichern(&self, s: TcpStream) -> Result<RustlsFtpStream> {
        let t = self.zeitlimit;
        zeitlimits(&s, t);
        begruessung_pruefen(&s, &self.ort(), t)?;
        let roh = RustlsFtpStream::connect_with_stream(s).map_err(ftp_fehler)?;
        *self.gesehen.lock().expect("Zertifikatsstand") = Gesehen::default();
        let mut ftp = roh.into_secure(RustlsConnector::from(self.tls.clone()), &self.ziel.host).map_err(|e| self.tls_fehler(e))?;
        ftp.set_passive_nat_workaround(true);
        let mut ftp = ftp.passive_stream_builder(move |a| datenkanal(a, t));
        ftp.login(self.ziel.benutzer.as_str(), self.passwort.as_str()).map_err(anmeldefehler)?;
        ftp.transfer_type(FileType::Binary).map_err(ftp_fehler)?;
        ftp.cwd(&self.ziel.pfad)
            .map_err(|e| StellwerkError::NichtGefunden(format!("Serverpfad {} auf {}: {e}", self.ziel.pfad, self.ort())))?;
        Ok(ftp)
    }

    /// Fehler beim Sichern: Zertifikat abgelehnt, Server ohne TLS oder TLS gescheitert.
    fn tls_fehler(&self, e: FtpError) -> StellwerkError {
        let g = self.gesehen.lock().expect("Zertifikatsstand").clone();
        match (e, g.abgelehnt) {
            (FtpError::UnexpectedResponse(r), _) => StellwerkError::Verbindung(format!(
                "{} bietet kein TLS an ({}) – unverschlüsseltes FTP wird nicht unterstützt",
                self.ort(),
                antwort(&r)
            )),
            (_, Some(grund)) => StellwerkError::Zertifikat { meldung: grund, fingerprint: g.fingerprint },
            (e, _) => ftp_fehler(e),
        }
    }

    /// „Verbindung testen“: TLS, Anmeldung, Serverpfad, Datenkanal und Fähigkeiten.
    /// Liest nur – auf dem Server wird nichts verändert.
    pub fn verbindung_testen(&self) -> Result<Verbindungstest> {
        let mut ftp = self.anmelden()?;
        let begruessung = ftp.get_welcome_msg().map(begruessung_text);
        let verzeichnis = ftp.pwd().map_err(ftp_fehler)?;
        let eintraege = ftp.nlst(None).map_err(ftp_fehler)?.iter().filter(ist_eintrag).count();
        let feat = ftp.feat().unwrap_or_default();
        let chmod = match ftp.site("HELP") {
            Ok(r) => Some(nennt_chmod(&r.body)),
            Err(FtpError::UnexpectedResponse(r)) if r.status == Status::Help => Some(nennt_chmod(&r.body)),
            Err(_) => None,
        };
        let faehigkeiten = faehigkeiten_aus(&feat, chmod);
        let _ = ftp.quit();
        let g = self.gesehen.lock().expect("Zertifikatsstand").clone();
        Ok(Verbindungstest {
            begruessung,
            verzeichnis,
            eintraege,
            faehigkeiten,
            zertifikat: Zertifikatsstand { fingerprint: g.fingerprint, vom_system: g.system },
        })
    }
}

/// Fähigkeiten aus der `FEAT`-Antwort. `MLST` schließt `MLSD` ein (RFC 3659) –
/// ProFTPD z. B. meldet nur `MLST`.
fn faehigkeiten_aus(feat: &suppaftp::types::Features, chmod: Option<bool>) -> Faehigkeiten {
    let hat = |k: &str| feat.keys().any(|f| f.eq_ignore_ascii_case(k));
    let fortsetzen = feat
        .iter()
        .any(|(k, v)| k.eq_ignore_ascii_case("REST") && v.as_deref().is_some_and(|v| v.eq_ignore_ascii_case("STREAM")));
    Faehigkeiten { mlsd: hat("MLSD") || hat("MLST"), mfmt: hat("MFMT"), chmod, fortsetzen, utf8: hat("UTF8") }
}

/// Prüft die erste Antwort des Servers, bevor FTP beginnt – für verständliche Meldungen,
/// wenn am Port kein FTP-Server antwortet (z. B. SFTP/SSH) oder gar nichts kommt.
/// Gelesen wird nur vorab (`peek`), die Begrüßung bleibt für FTP erhalten.
fn begruessung_pruefen(s: &TcpStream, ort: &str, t: Duration) -> Result<()> {
    let mut anfang = [0u8; 64];
    let n = s.peek(&mut anfang).map_err(|e| {
        StellwerkError::Verbindung(format!("{ort}: keine Begrüßung vom Server innerhalb von {} s ({e})", t.as_secs()))
    })?;
    let text = String::from_utf8_lossy(&anfang[..n]);
    if n == 0 {
        Err(StellwerkError::Verbindung(format!("{ort}: Der Server hat die Verbindung sofort geschlossen.")))
    } else if text.starts_with("SSH-") {
        Err(StellwerkError::Verbindung(format!(
            "{ort}: Hier antwortet ein SFTP-Server (SSH), kein FTP-Server. Für FTPS ist meist Port 21 richtig."
        )))
    } else if !anfang[..3.min(n)].iter().all(u8::is_ascii_digit) {
        let erste = text.lines().next().unwrap_or_default().trim();
        Err(StellwerkError::Verbindung(format!(
            "{ort}: Hier antwortet kein FTP-Server (Antwort beginnt mit „{erste}“). Bitte Server und Port prüfen."
        )))
    } else {
        Ok(())
    }
}

/// Datenverbindung im passiven Modus – mit denselben Zeitlimits.
fn datenkanal(a: SocketAddr, t: Duration) -> suppaftp::FtpResult<TcpStream> {
    let s = TcpStream::connect_timeout(&a, t).map_err(FtpError::ConnectionError)?;
    zeitlimits(&s, t);
    Ok(s)
}

/// Lese- und Schreib-Zeitlimit; scheitert nur bei einer Dauer von 0, die es hier nicht gibt.
fn zeitlimits(s: &TcpStream, t: Duration) {
    let _ = s.set_read_timeout(Some(t));
    let _ = s.set_write_timeout(Some(t));
}

/// Fehler des Servers oder der Verbindung.
fn ftp_fehler(e: FtpError) -> StellwerkError {
    StellwerkError::Verbindung(match e {
        FtpError::UnexpectedResponse(r) => format!("Server antwortet {}", antwort(&r)),
        FtpError::BadResponse => "Antwort des Servers ist kein gültiges FTP".to_string(),
        e => e.to_string(),
    })
}

/// `530` bei der Anmeldung heißt: Benutzer oder Passwort abgelehnt.
fn anmeldefehler(e: FtpError) -> StellwerkError {
    match e {
        FtpError::UnexpectedResponse(r) if r.status == Status::NotLoggedIn => {
            StellwerkError::Anmeldung(format!("Benutzer oder Passwort abgelehnt ({})", antwort(&r)))
        }
        e => ftp_fehler(e),
    }
}

fn antwort(r: &suppaftp::types::Response) -> String {
    format!("{} {}", r.status.code(), String::from_utf8_lossy(&r.body).trim())
}

/// `.` und `..` zählen nicht als Einträge.
fn ist_eintrag(name: &&String) -> bool {
    name.as_str() != "." && name.as_str() != ".."
}

/// Begrüßung ohne Antwortcode (`220 ` bzw. `220-` je Zeile).
fn begruessung_text(w: &str) -> String {
    w.lines()
        .map(|z| z.strip_prefix("220").map_or(z, |r| r.trim_start_matches(['-', ' '])))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn nennt_chmod(text: &[u8]) -> bool {
    String::from_utf8_lossy(text).to_ascii_uppercase().contains("CHMOD")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_hin_und_zurueck() {
        let f: [u8; 32] = std::array::from_fn(|i| i as u8 * 7);
        let t = fingerprint_text(&f);
        assert_eq!(&t[..8], "00:07:0E");
        assert_eq!(fingerprint_bytes(&t), Some(f));
        assert_eq!(fingerprint_bytes(&t.to_lowercase().replace(':', " ")), Some(f));
        assert_eq!(fingerprint_bytes("AB:CD"), None);
        assert_eq!(fingerprint_bytes(&"g0".repeat(32)), None);
        assert_eq!(fingerprint_bytes(&format!("a{}a", "ä".repeat(31))), None, "Paar ist kein UTF-8");
    }

    #[test]
    fn fehler_ohne_besondere_bedeutung_bleiben_verbindungsfehler() {
        assert_eq!(anmeldefehler(FtpError::BadResponse).to_string(), "Verbindung: Antwort des Servers ist kein gültiges FTP");
        let weg = FtpError::ConnectionError(io::Error::new(io::ErrorKind::ConnectionReset, "zurückgesetzt"));
        assert!(ftp_fehler(weg).to_string().contains("zurückgesetzt"));
        let nein = suppaftp::types::Response::new(Status::NotLoggedIn, b"Login incorrect.".to_vec());
        assert_eq!(anmeldefehler(FtpError::UnexpectedResponse(nein)).kategorie(), "anmeldung");
        let abgelehnt = suppaftp::types::Response::new(Status::FileUnavailable, b"Keine Berechtigung".to_vec());
        assert_eq!(ftp_fehler(FtpError::UnexpectedResponse(abgelehnt)).to_string(), "Verbindung: Server antwortet 550 Keine Berechtigung");
    }

    #[test]
    fn faehigkeiten_aus_feat_von_proftpd() {
        // Echte Antwort von ProFTPD (manitu): MLST statt MLSD
        let mut f = suppaftp::types::Features::new();
        for k in ["AUTH", "MDTM", "MFMT", "PBSZ", "PROT", "SIZE", "UTF8"] {
            f.insert(k.into(), None);
        }
        f.insert("MLST".into(), Some("modify*;perm*;size*;type*;".into()));
        f.insert("REST".into(), Some("STREAM".into()));
        assert_eq!(
            faehigkeiten_aus(&f, Some(true)),
            Faehigkeiten { mlsd: true, mfmt: true, chmod: Some(true), fortsetzen: true, utf8: true }
        );
        let mut g = suppaftp::types::Features::new();
        g.insert("mlsd".into(), None);
        g.insert("REST".into(), Some("RANG".into()));
        assert_eq!(
            faehigkeiten_aus(&g, None),
            Faehigkeiten { mlsd: true, mfmt: false, chmod: None, fortsetzen: false, utf8: false }
        );
        assert!(!faehigkeiten_aus(&Default::default(), None).mlsd);
    }

    #[test]
    fn begruessung_ohne_antwortcode() {
        assert_eq!(begruessung_text("220-Willkommen\r\n220 bei Kim\r\n"), "Willkommen\nbei Kim");
        assert_eq!(begruessung_text("Hallo"), "Hallo");
    }

    #[test]
    fn chmod_in_site_help() {
        assert!(nennt_chmod(b"The following SITE commands are recognized: chmod umask"));
        assert!(!nennt_chmod(b"UMASK HELP"));
    }
}
