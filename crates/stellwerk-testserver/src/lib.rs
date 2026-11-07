//! FTPS-Server für die automatischen Tests (Übergabe v0.4 Kap. 7, Entscheidung Kim:
//! „Server im Test“). Läuft mit `cargo test` auf macOS und Linux, ohne Docker.
//!
//! - libunftp mit eigenem Zertifikat (selbstsigniert oder von einer Test-CA)
//! - Benutzer [`BENUTZER`] / Passwort [`PASSWORT`], Wurzel in einem Temp-Ordner
//! - TLS Pflicht auf Steuer- und Datenkanal (wie bei Hostern üblich), abschaltbar
//!   für den Test „Server ohne TLS“
//! - `SITE HELP` optional, um die Erkennung von `SITE CHMOD` zu prüfen
//!
//! libunftp meldet in `FEAT` u. a. `MLSD`, `MLST`, `SIZE`, `MDTM`, `REST STREAM`,
//! aber kein `MFMT`.

use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use libunftp::options::{FtpsRequired, TlsFlags};
use libunftp::ServerBuilder;
use rcgen::{BasicConstraints, CertificateParams, IsCa, Issuer, KeyPair};
use sha2::{Digest, Sha256};
use unftp_core::auth::{AuthenticationError, Authenticator, Credentials, Principal};
use unftp_sbe_fs::Filesystem;

pub const BENUTZER: &str = "kim";
pub const PASSWORT: &str = "geheim";

/// Wie der Server aufgebaut wird.
#[derive(Debug, Clone, Default)]
pub struct Aufbau {
    /// Zertifikat von einer Test-CA statt selbstsigniert (CA in [`TestServer::ca_der`]).
    pub mit_ca: bool,
    /// Kein TLS anbieten (Klartext-Server).
    pub ohne_tls: bool,
    /// Antwort auf `SITE HELP`: Antwortcode (200 oder 214) und Text, z. B.
    /// `(214, "CHMOD UMASK HELP")`; `None`: Befehl unbekannt.
    pub site_help: Option<(u32, &'static str)>,
    /// TLS 1.3 zusätzlich anbieten (Standard von libunftp: nur TLS 1.2).
    pub tls13: bool,
}

/// Ein laufender Server; endet mit dem Test (Laufzeit wird verworfen).
pub struct TestServer {
    pub port: u16,
    wurzel: tempfile::TempDir,
    /// SHA-256 des Serverzertifikats (DER), Großbuchstaben mit Doppelpunkten.
    pub fingerprint: String,
    /// DER der Test-CA, falls `mit_ca`.
    pub ca_der: Option<Vec<u8>>,
    _laufzeit: tokio::runtime::Runtime,
}

impl TestServer {
    pub fn wurzel(&self) -> &Path {
        self.wurzel.path()
    }

    /// Ordner unterhalb der Wurzel anlegen (z. B. `httpdocs`).
    pub fn ordner(&self, rel: &str) -> PathBuf {
        let p = self.wurzel.path().join(rel);
        std::fs::create_dir_all(&p).unwrap();
        p
    }
}

/// SHA-256 als `AB:CD:…`.
pub fn fingerprint(der: &[u8]) -> String {
    Sha256::digest(der).iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":")
}

#[derive(Debug)]
struct Zugang;

#[async_trait]
impl Authenticator for Zugang {
    async fn authenticate(&self, benutzer: &str, c: &Credentials) -> Result<Principal, AuthenticationError> {
        if benutzer != BENUTZER {
            return Err(AuthenticationError::BadUser);
        }
        if c.password.as_deref() != Some(PASSWORT) {
            return Err(AuthenticationError::BadPassword);
        }
        Ok(Principal { username: benutzer.into() })
    }
}

#[derive(Debug)]
struct SiteHelp(u32, &'static str);

#[async_trait]
impl<S, U> libunftp::options::SiteCommandHandler<S, U> for SiteHelp
where
    S: unftp_core::storage::StorageBackend<U> + 'static,
    S::Metadata: unftp_core::storage::Metadata,
    U: unftp_core::auth::UserDetail + 'static,
{
    async fn handle(&self, _: &libunftp::options::SiteCommandContext<S, U>) -> libunftp::options::Reply {
        use libunftp::options::{Reply, ReplyCode};
        let code = if self.0 == 200 { ReplyCode::CommandOkay } else { ReplyCode::HelpMessage };
        Reply::new(code, self.1)
    }
}

/// Startet einen Server auf `127.0.0.1` und wartet, bis er Verbindungen annimmt.
pub fn starten(a: Aufbau) -> TestServer {
    let wurzel = tempfile::tempdir().unwrap();
    let schluessel = tempfile::tempdir().unwrap().keep();
    let (zert_pem, key_pem, zert_der, ca_der) = zertifikat(a.mit_ca);
    let (zert_pfad, key_pfad) = (schluessel.join("server.pem"), schluessel.join("server.key"));
    std::fs::write(&zert_pfad, zert_pem).unwrap();
    std::fs::write(&key_pfad, key_pem).unwrap();

    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let laufzeit = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap();
    let root = wurzel.path().to_path_buf();
    let mut b = ServerBuilder::with_authenticator(Box::new(move || Filesystem::new(root.clone()).unwrap()), Arc::new(Zugang))
        .greeting("Stellwerk-Testserver")
        .passive_ports(50000..=60000);
    if !a.ohne_tls {
        b = b.ftps(zert_pfad, key_pfad).ftps_required(FtpsRequired::All, FtpsRequired::All);
        if a.tls13 {
            b = b.ftps_tls_flags(TlsFlags::LATEST_VERSIONS | TlsFlags::RESUMPTION_SESS_ID | TlsFlags::RESUMPTION_TICKETS);
        }
    }
    if let Some((code, text)) = a.site_help {
        b = b.site_command("HELP", SiteHelp(code, text));
    }
    let server = b.build().unwrap();
    laufzeit.spawn(async move { server.listen(format!("127.0.0.1:{port}")).await });

    let start = Instant::now();
    while TcpStream::connect(("127.0.0.1", port)).is_err() {
        assert!(start.elapsed() < Duration::from_secs(10), "Testserver startet nicht");
        std::thread::sleep(Duration::from_millis(20));
    }
    TestServer { port, wurzel, fingerprint: fingerprint(&zert_der), ca_der, _laufzeit: laufzeit }
}

/// (Zertifikat PEM, Schlüssel PEM, Zertifikat DER, CA DER)
fn zertifikat(mit_ca: bool) -> (String, String, Vec<u8>, Option<Vec<u8>>) {
    let namen = vec!["localhost".to_string(), "127.0.0.1".to_string()];
    let key = KeyPair::generate().unwrap();
    let params = CertificateParams::new(namen).unwrap();
    if !mit_ca {
        let c = params.self_signed(&key).unwrap();
        return (c.pem(), key.serialize_pem(), c.der().to_vec(), None);
    }
    let ca_key = KeyPair::generate().unwrap();
    let mut ca_params = CertificateParams::new(Vec::<String>::new()).unwrap();
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    let ca = ca_params.self_signed(&ca_key).unwrap();
    let issuer = Issuer::new(ca_params, ca_key);
    let c = params.signed_by(&key, &issuer).unwrap();
    (c.pem(), key.serialize_pem(), c.der().to_vec(), Some(ca.der().to_vec()))
}
