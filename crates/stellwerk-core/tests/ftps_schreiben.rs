//! Ausführung „Direkt“ in ein FTPS-Ziel gegen den Testserver (v0.4 Paket 3):
//! Vergleich (lokal gegen den Ordner des Servers) → Plan → Worker-Pool über FTPS
//! → erneuter Vergleich. Der Ordner des Servers liegt im Test lokal und lässt sich
//! direkt prüfen.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use stellwerk_core::ansicht::{stufe_vergleichen, VergleichsSitzung};
use stellwerk_core::config::{strecke_aus_toml, GlobalConfig};
use stellwerk_core::endpoint::{Ftps, Lokal};
use stellwerk_core::exec::{direkt_ausfuehren, LaufErgebnis, Optionen};
use stellwerk_core::model::{Dateimodus, FtpsEndpunkt, Rechte, Tls};
use stellwerk_core::plan::{plan_erstellen, OpArt, Plan};
use stellwerk_core::store::JournalStatus;
use stellwerk_testserver::{starten, Aufbau, TestServer, BENUTZER, PASSWORT};

const LAUF: &str = "f1";

struct Strecke {
    _q: tempfile::TempDir,
    q: PathBuf,
    s: TestServer,
    /// Ordner der Stufe auf dem Server (lokal sichtbar)
    z: PathBuf,
}

fn aufbau(a: Aufbau) -> Strecke {
    let q = tempfile::tempdir().unwrap();
    let s = starten(a);
    let z = s.ordner("httpdocs");
    Strecke { q: q.path().to_path_buf(), _q: q, s, z }
}

fn datei(wurzel: &Path, rel: &str, inhalt: &str) {
    let p = wurzel.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, inhalt).unwrap();
}

impl Strecke {
    /// Vergleich wie bei einer lokalen Stufe (Quelle ↔ Ordner des Servers) und Plan daraus.
    fn plan(&self, rechte: &Rechte) -> Plan {
        let toml = format!(
            "id = \"t\"\nname = \"T\"\nversion = 1\n[[stufen]]\nid = \"s\"\nname = \"S\"\nquelle = {{ typ = \"lokal\", pfad = \"{}\" }}\nziel = {{ typ = \"lokal\", pfad = \"{}\" }}\n",
            self.q.display(),
            self.z.display()
        );
        let s = strecke_aus_toml(&toml, Path::new("t.toml")).unwrap();
        let v = stufe_vergleichen(&s, "s", &GlobalConfig::default(), &AtomicBool::new(false), &mut |_| {}).unwrap();
        plan_erstellen(VergleichsSitzung::neu(v.vergleich).eintraege(), rechte).unwrap()
    }

    fn ftps(&self) -> Ftps {
        self.ftps_mit("/httpdocs/")
    }

    /// Serverpfad mit oder ohne Schrägstrich am Ende.
    fn ftps_mit(&self, pfad: &str) -> Ftps {
        let z = FtpsEndpunkt {
            host: "127.0.0.1".into(),
            port: self.s.port,
            pfad: pfad.into(),
            benutzer: BENUTZER.into(),
            tls: Tls::Explizit,
            konto_ref: "k".into(),
            zertifikat: Some(self.s.fingerprint.clone()),
        };
        Ftps::new(&z, PASSWORT).unwrap()
    }

    fn laufen(&self, plan: &Plan, ziel: &Ftps, worker: usize) -> (LaufErgebnis, Vec<(usize, JournalStatus, Option<String>)>) {
        self.laufen_mit(plan, ziel, worker, &AtomicBool::new(false))
    }

    fn laufen_mit(&self, plan: &Plan, ziel: &Ftps, worker: usize, abbruch: &AtomicBool) -> (LaufErgebnis, Vec<(usize, JournalStatus, Option<String>)>) {
        let mut journal = Vec::new();
        let opt = Optionen { worker, bei_fehler_stoppen: false, fortschritt_abstand: Duration::ZERO };
        let erg = direkt_ausfuehren(plan, &Lokal::new(&self.q), ziel, LAUF, &opt, abbruch, &mut |o, st, m, _| {
            journal.push((o.nr, st, m.map(String::from)));
            Ok(())
        }, &mut |_| {});
        (erg, journal)
    }

    /// Nichts mehr zu tun (Inhalte gleich; Zeit vergleicht der lokale Vergleich bei Bedarf per Hash).
    fn alles_gleich(&self) {
        let p = self.plan(&Rechte::NichtSetzen);
        assert!(p.operationen.is_empty(), "{:?}", p.operationen);
    }

    fn keine_temporaerdateien(&self) {
        let rest: Vec<_> = walk(&self.z).into_iter().filter(|p| p.to_string_lossy().contains(".stw-")).collect();
        assert!(rest.is_empty(), "{rest:?}");
    }
}

fn walk(d: &Path) -> Vec<PathBuf> {
    let mut v = Vec::new();
    for e in fs::read_dir(d).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            v.extend(walk(&p));
        }
        v.push(p);
    }
    v
}

fn modus(p: &Path) -> u32 {
    fs::metadata(p).unwrap().permissions().mode() & 0o777
}

fn webseite(st: &Strecke) {
    datei(&st.q, "index.php", "neu!");
    datei(&st.z, "index.php", "alt");
    datei(&st.q, "css/site.css", "c");
    datei(&st.z, "css/site.css", "c");
    datei(&st.q, "css/neu.css", "nnn");
    datei(&st.q, "bilder/neu/a.png", "aa");
    fs::create_dir_all(st.q.join("leer")).unwrap();
    datei(&st.z, "alt/unter/x.txt", "x");
    datei(&st.z, "weg.txt", "w");
}

#[test]
fn ftps_ziel_entspricht_danach_der_quelle_mit_jeder_workerzahl() {
    for (worker, pfad) in [(1, "/httpdocs/"), (3, "/httpdocs")] {
        let st = aufbau(Aufbau::default());
        webseite(&st);
        let plan = st.plan(&Rechte::NichtSetzen);
        let ftps = st.ftps_mit(pfad);
        let (erg, journal) = st.laufen(&plan, &ftps, worker);
        assert!(erg.vollstaendig(), "{erg:?}");
        assert_eq!(erg.bytes, 4 + 3 + 2);
        assert!(journal.iter().all(|(_, s, _)| *s == JournalStatus::Erledigt));
        assert_eq!(fs::read_to_string(st.z.join("index.php")).unwrap(), "neu!");
        assert!(st.z.join("leer").is_dir(), "leere Ordner werden angelegt");
        assert!(!st.z.join("alt").exists() && !st.z.join("weg.txt").exists());
        st.keine_temporaerdateien();
        st.alles_gleich();
        // Der Server überschreibt beim Umbenennen – gelernt
        assert_eq!(ftps.umbenennen_ueberschreibt(), Some(true), "Worker {worker}");
    }
}

#[test]
fn server_ueberschreibt_beim_umbenennen_nicht_rueckfall_und_gelernt() {
    let st = aufbau(Aufbau { umbenennen_ueberschreibt_nicht: true, ..Default::default() });
    datei(&st.q, "a.txt", "neu");
    datei(&st.z, "a.txt", "alt");
    datei(&st.q, "b.txt", "frisch");

    // Gemerkt war „ja“ – der Server lehnt ab, der Rückfall greift und korrigiert das
    let ftps = st.ftps().mit_umbenennen_ueberschreibt(Some(true));
    let (erg, _) = st.laufen(&st.plan(&Rechte::NichtSetzen), &ftps, 1);
    assert!(erg.vollstaendig(), "{erg:?}");
    assert_eq!(fs::read_to_string(st.z.join("a.txt")).unwrap(), "neu");
    assert_eq!(fs::read_to_string(st.z.join("b.txt")).unwrap(), "frisch", "neue Dateien betrifft das nicht");
    assert_eq!(ftps.umbenennen_ueberschreibt(), Some(false));

    // Mit gelerntem „nein“ gleich löschen und umbenennen
    datei(&st.q, "a.txt", "neuer");
    let (erg, _) = st.laufen(&st.plan(&Rechte::NichtSetzen), &ftps, 1);
    assert!(erg.vollstaendig(), "{erg:?}");
    assert_eq!(fs::read_to_string(st.z.join("a.txt")).unwrap(), "neuer");
    st.keine_temporaerdateien();
}

#[test]
fn neue_datei_inzwischen_am_server_wird_nicht_ueberschrieben() {
    let st = aufbau(Aufbau::default());
    datei(&st.q, "a.txt", "neu");
    let plan = st.plan(&Rechte::NichtSetzen);
    datei(&st.z, "a.txt", "fremd");
    let (erg, _) = st.laufen(&plan, &st.ftps(), 1);
    assert_eq!(erg.fehler.len(), 1);
    assert!(erg.fehler[0].meldung.contains("inzwischen vorhanden"), "{erg:?}");
    assert_eq!(fs::read_to_string(st.z.join("a.txt")).unwrap(), "fremd");
    st.keine_temporaerdateien();
}

#[test]
fn hochladen_oder_umbenennen_scheitert_temporaerdatei_wird_entfernt() {
    let st = aufbau(Aufbau::default());
    datei(&st.q, "neu/a.txt", "a");
    datei(&st.q, "b.txt", "b");
    let plan = st.plan(&Rechte::NichtSetzen);
    // Ordner „neu“ legt der Plan an – anstelle von b.txt steht inzwischen ein Ordner
    fs::create_dir(st.z.join("b.txt")).unwrap();
    let (erg, _) = st.laufen(&plan, &st.ftps(), 1);
    assert_eq!(erg.fehler.len(), 1, "{erg:?}");
    assert_eq!(erg.fehler[0].pfad.as_str(), "b.txt");
    st.keine_temporaerdateien();

    // Zielordner verschwindet nach dem Planen: Hochladen scheitert
    let st = aufbau(Aufbau::default());
    datei(&st.q, "unter/c.txt", "c");
    datei(&st.z, "unter/alt.txt", "x");
    let plan = st.plan(&Rechte::NichtSetzen);
    let nur_datei = Plan { operationen: plan.operationen.into_iter().filter(|o| o.art == OpArt::Kopieren).map(|mut o| { o.nr = 0; o.nach.clear(); o }).collect(), ..plan };
    fs::remove_dir_all(st.z.join("unter")).unwrap();
    let (erg, _) = st.laufen(&nur_datei, &st.ftps(), 1);
    assert_eq!(erg.fehler.len(), 1, "{erg:?}");
    st.keine_temporaerdateien();

    // Der Server verweigert das Umbenennen auf den Namen einer neuen Datei
    let st = aufbau(Aufbau { gesperrt: &["gesperrt.txt"], ..Default::default() });
    datei(&st.q, "gesperrt.txt", "g");
    let (erg, _) = st.laufen(&st.plan(&Rechte::NichtSetzen), &st.ftps(), 1);
    assert_eq!(erg.fehler.len(), 1, "{erg:?}");
    assert!(erg.fehler[0].meldung.contains("550"), "{erg:?}");
    assert!(!st.z.join("gesperrt.txt").exists());
    st.keine_temporaerdateien();
}

#[test]
fn rechte_per_site_chmod_oder_fehler_ohne() {
    let r = Rechte::Fest { ordner: Dateimodus::new(0o750), dateien: Dateimodus::new(0o640) };
    let st = aufbau(Aufbau { site_chmod: true, ..Default::default() });
    datei(&st.q, "web/a.php", "a");
    let (erg, _) = st.laufen(&st.plan(&r), &st.ftps(), 2);
    assert!(erg.vollstaendig(), "{erg:?}");
    assert_eq!([modus(&st.z.join("web")), modus(&st.z.join("web/a.php"))], [0o750, 0o640]);

    let st = aufbau(Aufbau::default());
    datei(&st.q, "a.php", "a");
    let (erg, _) = st.laufen(&st.plan(&r), &st.ftps(), 1);
    assert_eq!(erg.fehler.len(), 1, "{erg:?}");
    assert_eq!(erg.fehler[0].art, OpArt::RechteSetzen);
}

#[test]
fn loeschen_und_ordner_anlegen_sind_vorsichtig() {
    let st = aufbau(Aufbau { gesperrt: &["gesperrt.txt", "z.txt"], ..Default::default() });
    for f in ["weg.txt", "schon-weg.txt", "jetzt-ordner.txt", "gesperrt.txt", "voll/x.txt", "leer-weg/y.txt", "gesperrt-ordner/z.txt"] {
        datei(&st.z, f, "x");
    }
    datei(&st.q, "da/a.txt", "a");
    datei(&st.q, "datei-statt-ordner/b.txt", "b");
    let plan = st.plan(&Rechte::NichtSetzen);
    // Lage ändert sich nach dem Planen
    fs::remove_file(st.z.join("schon-weg.txt")).unwrap();
    fs::remove_file(st.z.join("jetzt-ordner.txt")).unwrap();
    fs::create_dir(st.z.join("jetzt-ordner.txt")).unwrap();
    datei(&st.z, "voll/neu.txt", "dazu");
    fs::remove_dir_all(st.z.join("leer-weg")).unwrap();
    fs::create_dir(st.z.join("da")).unwrap();
    datei(&st.z, "datei-statt-ordner", "f");

    let (erg, _) = st.laufen(&plan, &st.ftps(), 1);
    let mut fehler: Vec<_> = erg.fehler.iter().map(|f| f.pfad.as_str().to_string()).collect();
    fehler.sort();
    assert_eq!(
        fehler,
        ["datei-statt-ordner", "gesperrt-ordner/z.txt", "gesperrt.txt", "jetzt-ordner.txt", "voll"],
        "{erg:?}"
    );
    assert!(erg.fehler.iter().any(|f| f.meldung == "Verbindung: Server antwortet 550 Permission denied"), "Code nur einmal");
    assert_eq!(erg.uebersprungen, 2, "Ordner, deren Inhalt nicht gelöscht wurde");
    assert!(erg.fehler.iter().any(|f| f.meldung.contains("inzwischen ein Ordner")));
    assert!(!st.z.join("weg.txt").exists());
    assert_eq!(fs::read_to_string(st.z.join("da/a.txt")).unwrap(), "a", "vorhandener Ordner ist in Ordnung");
    assert!(st.z.join("voll/neu.txt").exists(), "Dazugekommenes bleibt");
}

#[test]
fn abbruch_waehrend_des_hochladens_raeumt_auf() {
    let st = aufbau(Aufbau::default());
    fs::write(st.q.join("gross.bin"), vec![1u8; 64 << 20]).unwrap();
    let plan = st.plan(&Rechte::NichtSetzen);
    let abbruch = AtomicBool::new(false);
    let ftps = st.ftps();
    let erg = std::thread::scope(|s| {
        s.spawn(|| {
            while !walk(&st.z).iter().any(|p| p.to_string_lossy().contains(".stw-")) {
                std::thread::sleep(Duration::from_millis(5));
            }
            abbruch.store(true, Ordering::SeqCst);
        });
        st.laufen_mit(&plan, &ftps, 1, &abbruch).0
    });
    assert!(erg.abgebrochen, "{erg:?}");
    assert!(!st.z.join("gross.bin").exists());
    st.keine_temporaerdateien();
}

#[test]
fn mfmt_wird_gesendet_und_ohne_unterstuetzung_uebergangen() {
    // libunftp kennt kein MFMT: die Übertragung gelingt trotzdem
    let st = aufbau(Aufbau::default());
    datei(&st.q, "a.txt", "a");
    let (erg, _) = st.laufen(&st.plan(&Rechte::NichtSetzen), &st.ftps().mfmt_erzwingen(), 1);
    assert!(erg.vollstaendig(), "{erg:?}");
    st.alles_gleich();
}

#[test]
fn ohne_verbindung_scheitert_jeder_auftrag() {
    let st = aufbau(Aufbau::default());
    datei(&st.q, "a.txt", "a");
    let z = FtpsEndpunkt {
        host: "127.0.0.1".into(),
        port: st.s.port,
        pfad: "/httpdocs".into(),
        benutzer: BENUTZER.into(),
        tls: Tls::Explizit,
        konto_ref: "k".into(),
        zertifikat: Some(st.s.fingerprint.clone()),
    };
    let (erg, _) = st.laufen(&st.plan(&Rechte::NichtSetzen), &Ftps::new(&z, "falsch").unwrap(), 1);
    assert_eq!(erg.fehler.len(), 1);
    assert!(erg.fehler[0].meldung.contains("Anmeldung"), "{erg:?}");
}
