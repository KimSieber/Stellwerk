//! Samba-Server für die automatischen Tests (v0.7.1) – ohne Docker und ohne
//! Administratorrechte: `smbd` läuft als eigener Prozess des Testbenutzers auf
//! einem freien Port, mit Konfiguration, Benutzerdatenbank und Freigaben in einem
//! Temp-Ordner.
//!
//! Voraussetzung: Samba ist installiert (macOS: `brew install samba`, Linux:
//! Paket `samba`). Gesucht wird in den üblichen Ordnern oder unter
//! `STELLWERK_SAMBA` (Ordner mit `smbd` und `pdbedit`). Homebrew nennt Sambas
//! Server `samba-dot-org-smbd` – das `smbd` von macOS (/usr/sbin) ist ein anderes
//! Programm und wird dort nie verwendet.
//!
//! Freigaben: `test` (beschreibbar) und `nurlesen` (schreibgeschützt).

use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub use crate::PASSWORT;

/// Ein laufender Samba-Server; endet mit dem Test.
pub struct SambaServer {
    pub port: u16,
    /// Anmeldename (der Benutzer, unter dem die Tests laufen).
    pub benutzer: String,
    ordner: tempfile::TempDir,
    prozess: Child,
}

impl Drop for SambaServer {
    fn drop(&mut self) {
        // smbd läuft in einer eigenen Prozessgruppe (samt seiner Kindprozesse) –
        // nur diese wird beendet, nie die des Tests
        let gruppe = format!("-{}", self.prozess.id());
        // Ist smbd schon beendet, meldet kill nur „not permitted“ – ohne Bedeutung
        let _ = Command::new("kill").args(["-KILL", "--", &gruppe]).stderr(Stdio::null()).status();
        let _ = self.prozess.kill();
        let _ = self.prozess.wait();
    }
}

impl SambaServer {
    /// Wurzel der beschreibbaren Freigabe `test`.
    pub fn wurzel(&self) -> PathBuf {
        self.ordner.path().join("test")
    }

    /// Wurzel der schreibgeschützten Freigabe `nurlesen`.
    pub fn nurlesen(&self) -> PathBuf {
        self.ordner.path().join("nurlesen")
    }

    /// Ordner unterhalb der Freigabe `test` anlegen.
    pub fn ordner(&self, rel: &str) -> PathBuf {
        let p = self.wurzel().join(rel);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

}

/// Sucht ein Samba-Programm unter einem seiner Namen.
fn programm(namen: &[&str]) -> PathBuf {
    let eigene = std::env::var_os("STELLWERK_SAMBA").map(PathBuf::from);
    // Auf macOS ist /usr/sbin/smbd Apples eigener Server – nicht Samba
    let system: &[&str] = if cfg!(target_os = "macos") { &[] } else { &["/usr/sbin", "/usr/bin"] };
    let ordner = ["/opt/homebrew/sbin", "/opt/homebrew/bin", "/usr/local/sbin", "/usr/local/bin"];
    let ordner: Vec<PathBuf> = eigene.into_iter().chain(ordner.iter().chain(system).map(PathBuf::from)).collect();
    namen
        .iter()
        .flat_map(|n| ordner.iter().map(move |o| o.join(n)))
        .find(|p| p.is_file())
        .unwrap_or_else(|| panic!("Samba-Programm „{}“ nicht gefunden – bitte Samba installieren (macOS: brew install samba)", namen[0]))
}

fn freier_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

fn benutzer() -> String {
    let aus = Command::new("id").arg("-un").output().expect("id -un");
    String::from_utf8(aus.stdout).unwrap().trim().to_string()
}

/// Umgebungsvariable: Ordner, in den Samba sein Protokoll schreibt (zur Fehlersuche;
/// bleibt nach dem Test erhalten).
const PROTOKOLL: &str = "STELLWERK_SAMBA_PROTOKOLL";

fn protokoll_ordner(d: &Path) -> PathBuf {
    match std::env::var_os(PROTOKOLL) {
        Some(o) => {
            let o = PathBuf::from(o).join(d.file_name().unwrap());
            std::fs::create_dir_all(&o).unwrap();
            o
        }
        None => d.to_path_buf(),
    }
}

fn konfiguration(d: &Path, port: u16) -> String {
    let p = |n: &str| d.join(n).display().to_string();
    format!(
        "[global]
  smb ports = {port}
  bind interfaces only = yes
  interfaces = 127.0.0.1
  lock directory = {lock}
  state directory = {state}
  cache directory = {cache}
  pid directory = {pid}
  private dir = {private}
  ncalrpc dir = {ncalrpc}
  log file = {log}
  passdb backend = tdbsam:{private}/passdb.tdb
  disable spoolss = yes
  load printers = no
  server role = standalone server
  server min protocol = SMB2_10
  unix extensions = no
  map archive = no
  # Rechte nur nach Unix-Modus prüfen, keine Zusatzattribute (macOS: Ordner anlegen)
  nt acl support = no
  store dos attributes = no
  ea support = no
  map hidden = no
  map system = no
  inherit acls = no
  log level = {stufe}
[test]
  path = {test}
  read only = no
[nurlesen]
  path = {nurlesen}
  read only = yes
",
        lock = p("lock"),
        state = p("state"),
        cache = p("cache"),
        pid = p("pid"),
        private = p("private"),
        ncalrpc = p("ncalrpc"),
        log = protokoll_ordner(d).join("log.%m").display(),
        stufe = if std::env::var_os(PROTOKOLL).is_some() { 3 } else { 1 },
        test = p("test"),
        nurlesen = p("nurlesen"),
    )
}

/// Startet einen Samba-Server und wartet, bis er Verbindungen annimmt.
pub fn starten() -> SambaServer {
    let ordner = tempfile::tempdir().unwrap();
    let d = ordner.path().to_path_buf();
    let d = d.as_path();
    for n in ["lock", "state", "cache", "pid", "private", "ncalrpc", "test", "nurlesen"] {
        std::fs::create_dir_all(d.join(n)).unwrap();
    }
    let port = freier_port();
    let conf = d.join("smb.conf");
    std::fs::write(&conf, konfiguration(d, port)).unwrap();
    let benutzer = benutzer();

    let mut pdbedit = Command::new(programm(&["pdbedit"]))
        .args(["-s", conf.to_str().unwrap(), "-a", "-u", &benutzer, "-t"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("pdbedit starten");
    use std::io::Write;
    write!(pdbedit.stdin.take().unwrap(), "{PASSWORT}\n{PASSWORT}\n").unwrap();
    let aus = pdbedit.wait_with_output().unwrap();
    assert!(aus.status.success(), "pdbedit: {}", String::from_utf8_lossy(&aus.stderr));

    // Eigene Prozessgruppe schon beim Start; `--no-process-group` verhindert, dass
    // smbd sie selbst ändert. Samba schickt beim Beenden seiner ganzen Gruppe ein
    // SIGTERM – in der Gruppe des Tests träfe das auch cargo und npm. So bleibt alles,
    // was smbd startet (auch Hilfsprozesse), in dieser Gruppe.
    use std::os::unix::process::CommandExt;
    let prozess = Command::new(programm(&["samba-dot-org-smbd", "smbd"]))
        .args(["-s", conf.to_str().unwrap(), "--foreground", "--no-process-group", "--debug-stdout", "-d", "1"])
        .process_group(0)
        // Keine Standardeingabe: Am Terminal (fremde Prozessgruppe) liefe ein Lesen auf
        // einen Fehler, den smbd als „Eingabe geschlossen“ wertet und sich beendet
        .stdin(Stdio::null())
        .stdout(std::fs::File::create(d.join("smbd.out")).unwrap())
        .stderr(Stdio::null())
        .spawn()
        .expect("smbd starten");
    let server = SambaServer { port, benutzer, ordner, prozess };

    let bis = Instant::now() + Duration::from_secs(15);
    while TcpStream::connect(("127.0.0.1", port)).is_err() {
        assert!(Instant::now() < bis, "smbd nimmt keine Verbindung an: {}", std::fs::read_to_string(d.join("smbd.out")).unwrap_or_default());
        std::thread::sleep(Duration::from_millis(50));
    }
    server
}
