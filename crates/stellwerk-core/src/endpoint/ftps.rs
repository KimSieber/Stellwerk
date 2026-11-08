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
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
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

use super::{kopieren, temp_name, Geschrieben, QuellDatei, ZielEndpunkt, ZielVerbindung};
use crate::error::{Result, StellwerkError};
use crate::model::{Eintrag, EintragArt, FtpsEndpunkt, RegelTyp, RelPath, Warnung};
use crate::rules::RegelWerk;
use crate::scan::{aus_liste, ist_konfliktkopie, ScanErgebnis, ScanFehler};

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
    /// Gelernt: Überschreibt `RNTO` eine vorhandene Datei? (`None`: noch nicht bekannt.)
    /// Gemeinsam für alle Verbindungen dieses Ziels.
    umbenennen_ueberschreibt: Arc<Mutex<Option<bool>>>,
    /// `MFMT` auch versuchen, wenn `FEAT` es nicht nennt (Tests).
    mfmt_erzwingen: bool,
    /// Verzeichnisse nur mit `LIST` lesen, auch wenn der Server `MLSD` kann (Tests).
    nur_list: bool,
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
        Ok(Self {
            ziel: ziel.clone(),
            passwort: passwort.to_string(),
            tls: Arc::new(tls),
            gesehen,
            zeitlimit: ZEITLIMIT,
            umbenennen_ueberschreibt: Arc::new(Mutex::new(None)),
            mfmt_erzwingen: false,
            nur_list: false,
        })
    }

    /// Anderes Zeitlimit (Tests).
    pub fn zeitlimit(mut self, t: Duration) -> Self {
        self.zeitlimit = t;
        self
    }

    /// Bereits gelernt, ob `RNTO` überschreibt (aus `stufen_status`).
    pub fn mit_umbenennen_ueberschreibt(self, wert: Option<bool>) -> Self {
        *self.umbenennen_ueberschreibt.lock().expect("Gelerntes") = wert;
        self
    }

    /// Was in den Läufen über `RNTO` gelernt wurde – zum Speichern in `stufen_status`.
    pub fn umbenennen_ueberschreibt(&self) -> Option<bool> {
        *self.umbenennen_ueberschreibt.lock().expect("Gelerntes")
    }

    /// `MFMT` versuchen, auch wenn der Server es in `FEAT` nicht nennt – nur für Tests
    /// gegen Server ohne `MFMT` (der Fehler wird dann wie vorgesehen übergangen).
    pub fn mfmt_erzwingen(mut self) -> Self {
        self.mfmt_erzwingen = true;
        self
    }

    /// Verzeichnisse mit `LIST` statt `MLSD` lesen – nur für Tests des Rückfalls.
    pub fn nur_list(mut self) -> Self {
        self.nur_list = true;
        self
    }

    /// Liest den Serverpfad der Stufe vollständig ein (Erstvergleich, Vollabgleich).
    /// Liest nur. „Ignorieren“ wirkt wie beim lokalen Scan; ignorierte Ordner werden
    /// nicht betreten. Ist die Wurzel nicht lesbar, ist das ein Fehler (sonst sähe
    /// alles „neu“ aus); nicht lesbare Unterordner und Zeilen erscheinen als Lesefehler.
    pub fn auflisten(&self, regeln: &RegelWerk, abbruch: &AtomicBool, fortschritt: &mut dyn FnMut(usize)) -> Result<ScanErgebnis> {
        let mut ftp = self.anmelden()?;
        let mlsd = !self.nur_list && faehigkeiten_aus(&ftp.feat().unwrap_or_default(), None).mlsd;
        let mut liste = Vec::new();
        let mut fehler = Vec::new();
        let mut offen = vec![RelPath::wurzel()];
        while let Some(ordner) = offen.pop() {
            if abbruch.load(Ordering::Relaxed) {
                return Err(StellwerkError::Abbruch);
            }
            let abs = self.absolut(ordner.as_str());
            let zeilen = match if mlsd { ftp.mlsd(Some(&abs)) } else { ftp.list(Some(&abs)) } {
                Ok(z) => z,
                Err(e) if ordner.ist_wurzel() => return Err(StellwerkError::NichtGefunden(format!("Serverpfad {abs} nicht lesbar: {e}"))),
                Err(e) => {
                    fehler.push(ScanFehler { pfad: ordner.to_string(), meldung: format!("Ordner nicht lesbar: {e}") });
                    continue;
                }
            };
            for zeile in zeilen {
                let eintrag = match eintrag_aus(&ordner, &zeile, mlsd) {
                    Ok(e) => e,
                    Err(meldung) => {
                        fehler.push(ScanFehler { pfad: format!("{abs}: {zeile}"), meldung });
                        None
                    }
                };
                let Some(eintrag) = eintrag else { continue };
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

    /// Absoluter Serverpfad zu einem Pfad relativ zur Stufenwurzel.
    fn absolut(&self, p: &str) -> String {
        format!("{}/{p}", self.ziel.pfad.trim_end_matches('/'))
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

impl ZielEndpunkt for Ftps {
    /// Eigene Server-Verbindung je Worker; `MFMT` wird je Verbindung aus `FEAT` bestimmt.
    fn verbinden(&self) -> Result<Box<dyn ZielVerbindung + '_>> {
        let mut ftp = self.anmelden()?;
        let mfmt = self.mfmt_erzwingen || faehigkeiten_aus(&ftp.feat().unwrap_or_default(), None).mfmt;
        Ok(Box::new(FtpsVerbindung { ziel: self, ftp, mfmt, aus_dem_takt: false }))
    }
}

struct FtpsVerbindung<'a> {
    ziel: &'a Ftps,
    ftp: RustlsFtpStream,
    mfmt: bool,
    /// Nach `ABOR` senden viele Server zwei Antworten (426, 226) – die Steuerverbindung
    /// ist dann nicht mehr verlässlich und wird vor dem nächsten Befehl ersetzt.
    aus_dem_takt: bool,
}

impl FtpsVerbindung<'_> {
    /// Ersetzt eine aus dem Takt geratene Verbindung durch eine neue. Gelingt das nicht,
    /// bleibt die alte – das Aufräumen kann dann scheitern; eine liegen gebliebene
    /// Temporärdatei zeigt der nächste Vergleich.
    fn wieder_im_takt(&mut self) {
        if std::mem::take(&mut self.aus_dem_takt) {
            if let Ok(neu) = self.ziel.anmelden() {
                self.ftp = neu;
            }
        }
    }

    /// Ordner? (`CWD` gelingt) – alle Pfade sind absolut, das Arbeitsverzeichnis spielt keine Rolle.
    fn ist_ordner(&mut self, abs: &str) -> bool {
        self.ftp.cwd(abs).is_ok()
    }

    /// Datei vorhanden? (`SIZE` gelingt)
    fn ist_datei(&mut self, abs: &str) -> bool {
        self.ftp.size(abs).is_ok()
    }

    /// Hochladen nach `tmp`, Zeit setzen, umbenennen – ohne Aufräumen (macht der Aufrufer).
    fn hochladen(&mut self, q: QuellDatei, tmp: &str, ziel: &str, neu: bool, ort: &RelPath, abbruch: &AtomicBool) -> Result<Geschrieben> {
        let QuellDatei { mut inhalt, geaendert, .. } = q;
        let mut strom = self.ftp.put_with_stream(tmp).map_err(ftp_fehler)?;
        let geschrieben = match kopieren(&mut inhalt, &mut strom, abbruch, Path::new(ort.as_str()), Path::new(tmp)) {
            Ok(g) => g,
            Err(e) => {
                let _ = self.ftp.abort(strom);
                self.aus_dem_takt = true;
                return Err(e);
            }
        };
        strom.finish().map_err(ftp_fehler)?;
        if self.mfmt {
            // Ohne Erfolg bleibt die Zeit des Servers – das Manifest kennt die richtige
            let _ = self.ftp.custom_command(format!("MFMT {} {tmp}", mfmt_zeit(geaendert)), &[Status::File]);
        }
        self.umbenennen(tmp, ziel, neu)?;
        Ok(geschrieben)
    }

    /// `RNFR/RNTO`. Beim Ersetzen: Überschreibt der Server nicht, wird die alte Datei
    /// gelöscht und sofort umbenannt; das Ergebnis wird für die Stufe gelernt.
    fn umbenennen(&mut self, tmp: &str, ziel: &str, neu: bool) -> Result<()> {
        let gelernt = &self.ziel.umbenennen_ueberschreibt;
        let bekannt = *gelernt.lock().expect("Gelerntes");
        if !neu && bekannt == Some(false) {
            self.ftp.rm(ziel).map_err(ftp_fehler)?;
            return self.ftp.rename(tmp, ziel).map_err(ftp_fehler);
        }
        match self.ftp.rename(tmp, ziel) {
            Ok(()) => {
                if !neu {
                    *gelernt.lock().expect("Gelerntes") = Some(true);
                }
                Ok(())
            }
            Err(_) if !neu && self.ist_datei(ziel) => {
                self.ftp.rm(ziel).map_err(ftp_fehler)?;
                self.ftp.rename(tmp, ziel).map_err(ftp_fehler)?;
                *gelernt.lock().expect("Gelerntes") = Some(false);
                Ok(())
            }
            Err(e) => Err(ftp_fehler(e)),
        }
    }
}

impl ZielVerbindung for FtpsVerbindung<'_> {
    fn ordner_anlegen(&mut self, p: &RelPath) -> Result<()> {
        let abs = self.ziel.absolut(p.as_str());
        match self.ftp.mkdir(&abs) {
            Err(_) if self.ist_ordner(&abs) => Ok(()),
            r => r.map_err(ftp_fehler),
        }
    }

    fn datei_schreiben(&mut self, p: &RelPath, q: QuellDatei, neu: bool, lauf_id: &str, abbruch: &AtomicBool) -> Result<Geschrieben> {
        let ziel = self.ziel.absolut(p.as_str());
        if neu && self.ist_datei(&ziel) {
            return Err(StellwerkError::Vorhanden(format!("{ziel} – am Server inzwischen vorhanden, bitte neu vergleichen")));
        }
        let (ordner, name) = ziel.rsplit_once('/').expect("absoluter Pfad");
        let tmp = format!("{ordner}/{}", temp_name(name, lauf_id));
        let ergebnis = self.hochladen(q, &tmp, &ziel, neu, p, abbruch);
        if ergebnis.is_err() {
            self.wieder_im_takt();
            let _ = self.ftp.rm(&tmp);
        }
        ergebnis
    }

    fn rechte_setzen(&mut self, p: &RelPath, bits: u32) -> Result<()> {
        let abs = self.ziel.absolut(p.as_str());
        self.ftp.site(format!("CHMOD {bits:o} {abs}")).map(drop).map_err(ftp_fehler)
    }

    fn datei_loeschen(&mut self, p: &RelPath) -> Result<()> {
        let abs = self.ziel.absolut(p.as_str());
        match self.ftp.rm(&abs) {
            Ok(()) => Ok(()),
            Err(_) if self.ist_ordner(&abs) => {
                Err(StellwerkError::NichtUnterstuetzt(format!("{abs} ist inzwischen ein Ordner – bitte neu vergleichen")))
            }
            Err(_) if !self.ist_datei(&abs) => Ok(()),
            Err(e) => Err(ftp_fehler(e)),
        }
    }

    fn ordner_loeschen(&mut self, p: &RelPath) -> Result<()> {
        let abs = self.ziel.absolut(p.as_str());
        match self.ftp.rmdir(&abs) {
            Err(_) if !self.ist_ordner(&abs) => Ok(()),
            r => r.map_err(ftp_fehler),
        }
    }
}

/// Eintrag aus einer Zeile des Listings im Ordner `ordner`. `Ok(None)`: Zeile ohne
/// Eintrag (`.`, `..`, `total`). Namen mit `/` oder `..` (fehlerhafter Server) sind Fehler.
fn eintrag_aus(ordner: &RelPath, zeile: &str, mlsd: bool) -> std::result::Result<Option<Eintrag>, String> {
    let roh = if mlsd { mlsd_zeile(zeile)? } else { list_zeile(zeile)? };
    let Some((name, art, groesse, mtime_ns)) = roh else { return Ok(None) };
    let pfad = match ordner.join(&name) {
        Ok(p) if !name.contains('/') => p,
        _ => return Err(format!("Name „{name}“ ist als Pfad ungültig")),
    };
    let warnungen = ist_konfliktkopie(&name).then_some(Warnung::Konfliktkopie).into_iter().collect();
    Ok(Some(Eintrag { pfad, art, groesse, mtime_ns, nur_online: false, hash: None, warnungen }))
}

/// Eine Zeile aus `MLSD` (RFC 3659): `fakt=wert;fakt=wert; name`.
/// `Ok(None)` für `cdir`/`pdir`; Ergebnis: (Name, Art, Größe, Zeit in ns).
type Zeile = (String, EintragArt, u64, Option<i64>);

fn mlsd_zeile(zeile: &str) -> std::result::Result<Option<Zeile>, String> {
    let (fakten, name) = zeile.split_once(' ').ok_or_else(|| "MLSD-Zeile ohne Namen".to_string())?;
    let mut art = None;
    let mut groesse = 0;
    let mut mtime_ns = None;
    for fakt in fakten.split(';').filter(|f| !f.is_empty()) {
        let (k, v) = fakt.split_once('=').unwrap_or((fakt, ""));
        match k.to_ascii_lowercase().as_str() {
            "type" => art = Some(v.to_ascii_lowercase()),
            "size" | "sizd" => groesse = v.parse().map_err(|_| format!("Größe „{v}“ nicht lesbar"))?,
            "modify" => mtime_ns = mlsd_zeit(v),
            _ => {}
        }
    }
    let art = match art.as_deref() {
        Some("file") => EintragArt::Datei,
        Some("dir") => EintragArt::Ordner,
        Some("cdir" | "pdir") => return Ok(None),
        Some(t) if t.contains("link") => EintragArt::Symlink,
        _ => return Err("MLSD-Zeile ohne bekannten Typ".into()),
    };
    let groesse = if art == EintragArt::Datei { groesse } else { 0 };
    Ok(Some((name.to_string(), art, groesse, mtime_ns)))
}

/// Eine Zeile aus `LIST` im Unix-Format (`-rw-r--r-- 1 u g 123 Nov 14 22:13 name`).
/// Die Zeit ist dort ungenau und wird nicht verwendet.
fn list_zeile(zeile: &str) -> std::result::Result<Option<Zeile>, String> {
    let unlesbar = || format!("LIST-Zeile nicht lesbar: {zeile}");
    if zeile.starts_with("total ") {
        return Ok(None);
    }
    let art = match zeile.chars().next() {
        Some('-') => EintragArt::Datei,
        Some('d') => EintragArt::Ordner,
        Some('l') => EintragArt::Symlink,
        _ => return Err(unlesbar()),
    };
    // Rechte, Links, Benutzer, Gruppe, Größe, 3 × Datum – dann der Name (darf Leerzeichen enthalten)
    let mut rest = zeile;
    let mut felder = Vec::new();
    for _ in 0..8 {
        let t = rest.trim_start();
        let ende = t.find(' ').ok_or_else(unlesbar)?;
        felder.push(&t[..ende]);
        rest = &t[ende..];
    }
    let name = rest.trim_start_matches(' ');
    let name = if art == EintragArt::Symlink { name.split(" -> ").next().unwrap_or(name) } else { name };
    if matches!(name, "" | "." | "..") {
        return Ok(None);
    }
    let groesse = if art == EintragArt::Datei { felder[4].parse().map_err(|_| unlesbar())? } else { 0 };
    Ok(Some((name.to_string(), art, groesse, None)))
}

/// `JJJJMMTTHHMMSS[.sss]` (UTC) in Nanosekunden seit 1970.
fn mlsd_zeit(v: &str) -> Option<i64> {
    let (haupt, bruch) = v.split_once('.').unwrap_or((v, ""));
    if haupt.len() != 14 || !haupt.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n = |a: usize, b: usize| haupt[a..b].parse::<i64>().expect("nur Ziffern");
    let (y, m, d) = (n(0, 4), n(4, 6), n(6, 8));
    // Tage seit 1970 aus dem Kalenderdatum (Verfahren nach H. Hinnant)
    let y = y - i64::from(m <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let tage = era * 146_097 + doe - 719_468;
    let sek = tage * 86_400 + n(8, 10) * 3600 + n(10, 12) * 60 + n(12, 14);
    let ns: i64 = format!("{bruch:0<9}")[..9].parse().unwrap_or(0);
    Some(sek * 1_000_000_000 + ns)
}

/// Zeit für `MFMT` in UTC: `JJJJMMTTHHMMSS` (vor 1970: 1970).
fn mfmt_zeit(t: std::time::SystemTime) -> String {
    let s = t.duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (tag, rest) = ((s / 86_400) as i64, s % 86_400);
    // Kalenderdatum aus Tagen seit 1970 (Verfahren nach H. Hinnant)
    let z = tag + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}{m:02}{d:02}{:02}{:02}{:02}", rest / 3600, rest % 3600 / 60, rest % 60)
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
    let code = r.status.code().to_string();
    let text = String::from_utf8_lossy(&r.body);
    let text = text.trim();
    // Manche Antworten enthalten den Code schon am Anfang
    if text.starts_with(&code) { text.to_string() } else { format!("{code} {text}") }
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
        let mit_code = suppaftp::types::Response::new(Status::FileUnavailable, b"550 Keine Berechtigung\r\n".to_vec());
        assert_eq!(ftp_fehler(FtpError::UnexpectedResponse(mit_code)).to_string(), "Verbindung: Server antwortet 550 Keine Berechtigung");
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
    fn mfmt_zeit_in_utc() {
        use std::time::{Duration, UNIX_EPOCH};
        assert_eq!(mfmt_zeit(UNIX_EPOCH), "19700101000000");
        assert_eq!(mfmt_zeit(UNIX_EPOCH + Duration::from_secs(1_700_000_000)), "20231114221320");
        assert_eq!(mfmt_zeit(UNIX_EPOCH + Duration::from_secs(951_782_400)), "20000229000000", "Schalttag");
        assert_eq!(mfmt_zeit(UNIX_EPOCH + Duration::from_secs(4_107_542_399)), "21000228235959");
        assert_eq!(mfmt_zeit(UNIX_EPOCH - Duration::from_secs(5)), "19700101000000");
    }

    #[test]
    fn eintrag_aus_zeilen() {
        let o = RelPath::new("web").unwrap();
        let e = eintrag_aus(&o, "type=file;size=3; a (conflicted copy).txt", true).unwrap().unwrap();
        assert_eq!(e.pfad.as_str(), "web/a (conflicted copy).txt");
        assert_eq!((e.art, e.groesse), (EintragArt::Datei, 3));
        assert_eq!(e.warnungen, vec![Warnung::Konfliktkopie]);
        let e = eintrag_aus(&RelPath::wurzel(), "drwxr-xr-x 2 u g 4096 Jan 1 2024 css", false).unwrap().unwrap();
        assert_eq!((e.pfad.as_str(), e.art, e.warnungen.len()), ("css", EintragArt::Ordner, 0));
        assert_eq!(eintrag_aus(&o, "type=cdir; .", true), Ok(None));
        assert!(eintrag_aus(&o, "kaputt", true).is_err());
        assert!(eintrag_aus(&o, "kaputt", false).is_err());
        assert!(eintrag_aus(&o, "type=file;size=1; ..", true).unwrap_err().contains("ungültig"));
        assert!(eintrag_aus(&o, "type=file;size=1; a/b", true).unwrap_err().contains("ungültig"));
    }

    #[test]
    fn mlsd_zeilen() {
        let z = |s| mlsd_zeile(s).unwrap();
        assert_eq!(
            z("type=file;size=1234;modify=20231114221320;perm=r; index.php"),
            Some(("index.php".into(), EintragArt::Datei, 1234, Some(1_700_000_000_000_000_000)))
        );
        assert_eq!(z("Type=dir;Size=4096;Modify=20231114221320.5; mit Leer zeichen"), Some(("mit Leer zeichen".into(), EintragArt::Ordner, 0, Some(1_700_000_000_500_000_000))));
        assert_eq!(z("type=cdir; ."), None);
        assert_eq!(z("type=pdir; .."), None);
        assert_eq!(z("type=OS.unix=slink:/x;modify=kaputt; link"), Some(("link".into(), EintragArt::Symlink, 0, None)));
        assert_eq!(z("type=file;x;sizd=7; a"), Some(("a".into(), EintragArt::Datei, 7, None)));
        assert!(mlsd_zeile("ohneleerzeichen").is_err());
        assert!(mlsd_zeile("size=1; a").is_err());
        assert!(mlsd_zeile("type=file;size=x; a").is_err());
    }

    #[test]
    fn mlsd_zeit_auch_vor_maerz_und_schaltjahr() {
        assert_eq!(mlsd_zeit("19700101000000"), Some(0));
        assert_eq!(mlsd_zeit("20000229000000"), Some(951_782_400_000_000_000));
        assert_eq!(mlsd_zeit("20240115120000.123"), Some(1_705_320_000_123_000_000));
        assert_eq!(mlsd_zeit("2024011512000"), None);
        assert_eq!(mlsd_zeit("2024011512000x"), None);
    }

    #[test]
    fn list_zeilen() {
        let z = |s| list_zeile(s).unwrap();
        assert_eq!(z("-rw-r--r--   1 kim  www      1234 Nov 14 22:13 index.php"), Some(("index.php".into(), EintragArt::Datei, 1234, None)));
        assert_eq!(z("drwxr-xr-x 2 kim www 4096 Jan  1  2024 mit  zwei Leer"), Some(("mit  zwei Leer".into(), EintragArt::Ordner, 0, None)));
        assert_eq!(z("lrwxrwxrwx 1 kim www 7 Jan 1 2024 link -> ziel"), Some(("link".into(), EintragArt::Symlink, 0, None)));
        assert_eq!(z("total 12"), None);
        assert_eq!(z("drwxr-xr-x 2 kim www 4096 Jan 1 2024 ."), None);
        assert!(list_zeile("01-01-24  10:00AM <DIR> windows").is_err());
        assert!(list_zeile("-rw-r--r-- 1 kim").is_err());
        assert!(list_zeile("-rw-r--r-- 1 kim www gross Nov 14 22:13 a").is_err());
        assert!(list_zeile("").is_err());
    }

    #[test]
    fn chmod_in_site_help() {
        assert!(nennt_chmod(b"The following SITE commands are recognized: chmod umask"));
        assert!(!nennt_chmod(b"UMASK HELP"));
    }
}
