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

use std::fmt::Debug;
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
use unftp_core::storage::{Error as SError, ErrorKind as SErrorKind, Fileinfo, Result as SResult, StorageBackend};
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
    /// `RNTO` auf eine vorhandene Datei ablehnen (wie manche Server).
    pub umbenennen_ueberschreibt_nicht: bool,
    /// `SITE CHMOD <oktal> <pfad>` anbieten (wirkt auf den Ordner des Servers).
    pub site_chmod: bool,
    /// Namen (exakt), die weder gelöscht (`DELE`/`RMD`), Ziel einer Umbenennung
    /// (`RNTO`) noch als Ordner gelistet werden dürfen.
    pub gesperrt: &'static [&'static str],
    /// Ordnernamen (exakt), in denen weder Dateien hochgeladen (`STOR`) noch
    /// Ordner angelegt (`MKD`) werden dürfen – „550 Permission denied“ wie bei
    /// einem FTP-Benutzer ohne Schreibrechte.
    pub schreibgeschuetzt: &'static [&'static str],
}

/// Ein laufender Server; endet mit dem Test (Laufzeit wird verworfen).
pub struct TestServer {
    pub port: u16,
    wurzel: tempfile::TempDir,
    /// SHA-256 des Serverzertifikats (DER), Großbuchstaben mit Doppelpunkten.
    pub fingerprint: String,
    /// DER der Test-CA, falls `mit_ca`.
    pub ca_der: Option<Vec<u8>>,
    laufzeit: Option<tokio::runtime::Runtime>,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        // Erst den Server beenden, dann die Sperre freigeben
        drop(self.laufzeit.take());
        freigeben();
    }
}

static NACHEINANDER: std::sync::Mutex<()> = std::sync::Mutex::new(());

thread_local! {
    /// Anzahl Server dieses Tests und die Sperre, solange er welche hat –
    /// ein Test darf mehrere Server zugleich halten.
    static HALTER: std::cell::RefCell<(usize, Option<std::sync::MutexGuard<'static, ()>>)> = const { std::cell::RefCell::new((0, None)) };
}

fn sperren() {
    HALTER.with(|h| {
        let mut h = h.borrow_mut();
        if h.0 == 0 {
            h.1 = Some(NACHEINANDER.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
        }
        h.0 += 1;
    });
}

fn freigeben() {
    HALTER.with(|h| {
        let mut h = h.borrow_mut();
        h.0 -= 1;
        if h.0 == 0 {
            h.1 = None;
        }
    });
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
///
/// Tests mit Server laufen **nacheinander** (eine Sperre je Testprogramm, gehalten,
/// solange ein Test Server hat; mehrere Server in einem Test sind erlaubt):
/// libunftp bindet passive Ports mit `SO_REUSEADDR` – laufen mehrere Server und
/// Sitzungen gleichzeitig, kann unter Linux eine Datenverbindung bei der falschen
/// Sitzung landen (TLS meldet dann „corrupt message“). Echte Server betrifft das nicht.
pub fn starten(a: Aufbau) -> TestServer {
    sperren();
    let wurzel = tempfile::tempdir().unwrap();
    let schluessel = tempfile::tempdir().unwrap().keep();
    let (zert_pem, key_pem, zert_der, ca_der) = zertifikat(a.mit_ca);
    let (zert_pfad, key_pfad) = (schluessel.join("server.pem"), schluessel.join("server.key"));
    std::fs::write(&zert_pfad, zert_pem).unwrap();
    std::fs::write(&key_pfad, key_pem).unwrap();

    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let laufzeit = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap();
    let root = wurzel.path().canonicalize().unwrap();
    let (streng, verweigert, geschuetzt) = (a.umbenennen_ueberschreibt_nicht, a.gesperrt, a.schreibgeschuetzt);
    let backend_root = root.clone();
    let mut b = ServerBuilder::with_authenticator(
        Box::new(move || Backend { fs: Filesystem::new(backend_root.clone()).unwrap(), root: backend_root.clone(), streng, verweigert, geschuetzt }),
        Arc::new(Zugang),
    )
        .greeting("Stellwerk-Testserver")
        .passive_ports(20_000..=60_000);
    if !a.ohne_tls {
        b = b.ftps(zert_pfad, key_pfad).ftps_required(FtpsRequired::All, FtpsRequired::All);
        b = b.ftps_tls_flags(if a.tls13 { TlsFlags::LATEST_VERSIONS } else { TlsFlags::V1_2 });
    }
    if a.site_chmod {
        b = b.site_command("CHMOD", SiteChmod(root.clone()));
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
    TestServer { port, wurzel, fingerprint: fingerprint(&zert_der), ca_der, laufzeit: Some(laufzeit) }
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

/// Dateisystem des Servers; optional mit „strengem“ Umbenennen.
#[derive(Debug)]
struct Backend {
    fs: Filesystem,
    root: PathBuf,
    streng: bool,
    verweigert: &'static [&'static str],
    geschuetzt: &'static [&'static str],
}

impl Backend {
    /// Schreiben im Ordner von `p` erlaubt?
    fn darf_schreiben(&self, p: &Path) -> SResult<()> {
        let ordner = p.parent().and_then(Path::file_name).map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        if self.geschuetzt.contains(&ordner.as_str()) {
            return Err(SError::from(SErrorKind::PermissionDenied));
        }
        Ok(())
    }

    fn darf_loeschen(&self, p: &Path) -> SResult<()> {
        let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        if self.verweigert.contains(&name.as_str()) {
            return Err(SError::from(SErrorKind::PermissionDenied));
        }
        Ok(())
    }
}

fn im_root(root: &Path, p: &Path) -> PathBuf {
    root.join(p.strip_prefix("/").unwrap_or(p))
}

#[async_trait]
impl<U: unftp_core::auth::UserDetail> StorageBackend<U> for Backend {
    type Metadata = <Filesystem as StorageBackend<U>>::Metadata;

    fn enter(&mut self, u: &U) -> std::io::Result<()> {
        StorageBackend::<U>::enter(&mut self.fs, u)
    }
    fn supported_features(&self) -> u32 {
        StorageBackend::<U>::supported_features(&self.fs)
    }
    async fn metadata<P: AsRef<Path> + Send + Debug>(&self, u: &U, p: P) -> SResult<Self::Metadata> {
        self.fs.metadata(u, p).await
    }
    async fn list<P: AsRef<Path> + Send + Debug>(&self, u: &U, p: P) -> SResult<Vec<Fileinfo<PathBuf, Self::Metadata>>> {
        self.darf_loeschen(p.as_ref())?;
        self.fs.list(u, p).await
    }
    async fn get<P: AsRef<Path> + Send + Debug>(&self, u: &U, p: P, start: u64) -> SResult<Box<dyn tokio::io::AsyncRead + Send + Sync + Unpin>> {
        self.fs.get(u, p, start).await
    }
    async fn put<P: AsRef<Path> + Send + Debug, R: tokio::io::AsyncRead + Send + Sync + Unpin + 'static>(&self, u: &U, r: R, p: P, start: u64) -> SResult<u64> {
        self.darf_schreiben(p.as_ref())?;
        self.fs.put(u, r, p, start).await
    }
    async fn del<P: AsRef<Path> + Send + Debug>(&self, u: &U, p: P) -> SResult<()> {
        self.darf_loeschen(p.as_ref())?;
        self.fs.del(u, p).await
    }
    async fn mkd<P: AsRef<Path> + Send + Debug>(&self, u: &U, p: P) -> SResult<()> {
        self.darf_schreiben(p.as_ref())?;
        self.fs.mkd(u, p).await
    }
    async fn rename<P: AsRef<Path> + Send + Debug>(&self, u: &U, von: P, nach: P) -> SResult<()> {
        self.darf_loeschen(nach.as_ref())?;
        if self.streng && im_root(&self.root, nach.as_ref()).exists() {
            return Err(SError::from(SErrorKind::PermanentFileNotAvailable));
        }
        self.fs.rename(u, von, nach).await
    }
    async fn rmd<P: AsRef<Path> + Send + Debug>(&self, u: &U, p: P) -> SResult<()> {
        self.darf_loeschen(p.as_ref())?;
        self.fs.rmd(u, p).await
    }
    async fn cwd<P: AsRef<Path> + Send + Debug>(&self, u: &U, p: P) -> SResult<()> {
        self.fs.cwd(u, p).await
    }
}

/// `SITE CHMOD <oktal> <absoluter Pfad>` auf dem Ordner des Servers.
#[derive(Debug)]
struct SiteChmod(PathBuf);

#[async_trait]
impl<S, U> libunftp::options::SiteCommandHandler<S, U> for SiteChmod
where
    S: StorageBackend<U> + 'static,
    S::Metadata: unftp_core::storage::Metadata,
    U: unftp_core::auth::UserDetail + 'static,
{
    async fn handle(&self, ctx: &libunftp::options::SiteCommandContext<S, U>) -> libunftp::options::Reply {
        use libunftp::options::{Reply, ReplyCode};
        use std::os::unix::fs::PermissionsExt;
        let Some((modus, pfad)) = ctx.arguments.split_once(' ') else {
            return Reply::new(ReplyCode::ParameterSyntaxError, "SITE CHMOD <modus> <pfad>");
        };
        let ziel = im_root(&self.0, Path::new(pfad));
        match u32::from_str_radix(modus, 8).map(|m| std::fs::set_permissions(&ziel, std::fs::Permissions::from_mode(m))) {
            Ok(Ok(())) => Reply::new(ReplyCode::CommandOkay, "SITE CHMOD command successful"),
            _ => Reply::new(ReplyCode::FileError, "SITE CHMOD failed"),
        }
    }
}
