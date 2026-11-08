//! Vergleich einer Stufe mit FTPS-Ziel (v0.4 Paket 4): Server einlesen (MLSD/LIST),
//! Manifest abgleichen, Vergleich gegen das Manifest ohne Serverkontakt.

#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use filetime::{set_file_mtime, FileTime};
use stellwerk_core::ansicht::{
    manifest_abgleichen, stufe_vergleichen, stufe_vergleichen_entfernt, EntfernterStand, EntfernterVergleich, Phase,
};
use stellwerk_core::config::{strecke_aus_toml, GlobalConfig};
use stellwerk_core::diff::DiffStatus;
use stellwerk_core::endpoint::Ftps;
use stellwerk_core::model::{Endpunkt, Hash, RelPath, Strecke, Warnung};
use stellwerk_core::scan::ScanErgebnis;
use stellwerk_core::store::ManifestEintrag;
use stellwerk_core::StellwerkError;
use stellwerk_testserver::{starten, Aufbau, TestServer, BENUTZER, PASSWORT};

const T: i64 = 1_700_000_000_000_000_000;

struct Fall {
    _q: tempfile::TempDir,
    q: PathBuf,
    _s: TestServer,
    z: PathBuf,
    strecke: Strecke,
}

fn fall(a: Aufbau) -> Fall {
    let q = tempfile::tempdir().unwrap();
    let s = starten(a);
    let z = s.ordner("httpdocs");
    let toml = format!(
        "id = \"t\"\nname = \"T\"\nversion = 1\n[[stufen]]\nid = \"server\"\nname = \"S\"\nquelle = {{ typ = \"lokal\", pfad = \"{}\" }}\nziel = {{ typ = \"ftps\", host = \"127.0.0.1\", port = {}, pfad = \"/httpdocs\", benutzer = \"{BENUTZER}\", konto_ref = \"k\", zertifikat = \"{}\" }}\n",
        q.path().display(),
        s.port,
        s.fingerprint
    );
    let strecke = strecke_aus_toml(&toml, Path::new("t.toml")).unwrap();
    Fall { q: q.path().to_path_buf(), _q: q, _s: s, z, strecke }
}

fn datei(wurzel: &Path, rel: &str, inhalt: &str) {
    let p = wurzel.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, inhalt).unwrap();
    set_file_mtime(&p, FileTime::from_unix_time(1_700_000_000, 0)).unwrap();
}

impl Fall {
    fn ftps(&self) -> Ftps {
        let Endpunkt::Ftps(f) = &self.strecke.stufen[0].ziel else { unreachable!() };
        Ftps::new(f, PASSWORT).unwrap()
    }

    fn vergleichen(&self, stand: EntfernterStand) -> stellwerk_core::Result<EntfernterVergleich> {
        stufe_vergleichen_entfernt(&self.strecke, "server", &GlobalConfig::default(), stand, &AtomicBool::new(false), &mut |_| {})
    }
}

fn status(v: &EntfernterVergleich) -> Vec<(String, DiffStatus)> {
    v.vergleich.vergleich.eintraege.iter().map(|e| (e.pfad.as_str().to_string(), e.status)).collect()
}

fn webseite(f: &Fall) {
    datei(&f.q, "index.php", "neu!");
    datei(&f.z, "index.php", "alt");
    datei(&f.q, "gleich.css", "ccc");
    datei(&f.z, "gleich.css", "xyz"); // gleiche Größe, anderer Inhalt – ohne Hash nicht erkennbar
    datei(&f.q, "neu.txt", "n");
    datei(&f.z, "weg/x.txt", "x");
    datei(&f.z, ".git/objects/ab", "g"); // ignoriert (Standard), wird nicht betreten
}

#[test]
fn erster_vergleich_liest_den_server_mlsd_und_list() {
    for nur_list in [false, true] {
        let f = fall(Aufbau::default());
        webseite(&f);
        let ftps = if nur_list { f.ftps().nur_list() } else { f.ftps() };
        let v = f.vergleichen(EntfernterStand::Server { ftps: &ftps, manifest: &[] }).unwrap();
        assert_eq!(
            status(&v),
            [
                (".git".to_string(), DiffStatus::Ignoriert),
                ("gleich.css".into(), DiffStatus::Gleich),
                ("index.php".into(), DiffStatus::Geaendert),
                ("neu.txt".into(), DiffStatus::Neu),
                ("weg".into(), DiffStatus::Loeschen),
                ("weg/x.txt".into(), DiffStatus::Loeschen),
            ],
            "LIST: {nur_list}"
        );
        let gleich = &v.vergleich.vergleich.eintraege[1];
        assert_eq!(gleich.warnungen, vec![Warnung::InhaltNichtGeprueft]);
        assert!(v.vergleich.ziel.to_string_lossy().starts_with("ftps://127.0.0.1:"));
        assert!(v.vergleich.lesefehler.is_empty());
        // Neues Manifest: Stand des Servers, ohne Hashes
        let m = v.neues_manifest.unwrap();
        let pfade: Vec<_> = m.iter().map(|e| (e.pfad.as_str(), e.groesse, e.hash.is_none())).collect();
        assert_eq!(pfade, [(".git", 0, true), ("gleich.css", 3, true), ("index.php", 3, true), ("weg", 0, true), ("weg/x.txt", 1, true)]);
    }
}

#[test]
fn nicht_lesbares_wird_gemeldet_wurzel_ist_ein_fehler() {
    let f = fall(Aufbau { gesperrt: &["geheim", "httpdocs"], ..Default::default() });
    let e = f.vergleichen(EntfernterStand::Server { ftps: &f.ftps(), manifest: &[] }).unwrap_err();
    assert!(matches!(e, StellwerkError::NichtGefunden(ref m) if m.contains("nicht lesbar")), "{e}");

    // „.git“ ist ignoriert und wird nicht betreten – sonst gäbe es hier einen Lesefehler mehr
    let f = fall(Aufbau { gesperrt: &["geheim", ".git"], ..Default::default() });
    datei(&f.z, "geheim/a.txt", "a");
    datei(&f.z, ".git/HEAD", "h");
    datei(&f.z, "zeilen\numbruch", "u");
    for (ftps, zeile) in [(f.ftps(), "ohne Namen"), (f.ftps().nur_list(), "LIST-Zeile nicht lesbar")] {
        let v = f.vergleichen(EntfernterStand::Server { ftps: &ftps, manifest: &[] }).unwrap();
        let fehler: Vec<_> = v.vergleich.lesefehler.iter().map(|l| l.meldung.clone()).collect();
        assert_eq!(fehler.len(), 2, "{fehler:?}");
        assert!(fehler.iter().any(|m| m.starts_with("Ordner nicht lesbar")));
        assert!(fehler.iter().any(|m| m.contains(zeile)), "Zeilenumbruch im Namen: {fehler:?}");
    }
}

#[test]
fn abbruch_waehrend_des_einlesens() {
    let f = fall(Aufbau::default());
    datei(&f.z, "a/1.txt", "1");
    datei(&f.z, "b/2.txt", "2");
    let abbruch = AtomicBool::new(false);
    let ftps = f.ftps();
    let e = stufe_vergleichen_entfernt(
        &f.strecke,
        "server",
        &GlobalConfig::default(),
        EntfernterStand::Server { ftps: &ftps, manifest: &[] },
        &abbruch,
        &mut |fs| {
            if fs.phase == Phase::Ziel {
                abbruch.store(true, Ordering::SeqCst);
            }
        },
    )
    .unwrap_err();
    assert!(matches!(e, StellwerkError::Abbruch), "{e}");
}

fn m(pfad: &str, groesse: u64, inhalt: Option<&str>, mtime_ns: Option<i64>) -> ManifestEintrag {
    ManifestEintrag {
        pfad: RelPath::new(pfad).unwrap(),
        ist_ordner: inhalt.is_none() && groesse == 0,
        groesse,
        hash: inhalt.map(|i| Hash(*blake3::hash(i.as_bytes()).as_bytes())),
        mtime_ns,
        lauf_id: Some("l1".into()),
    }
}

#[test]
fn vergleich_gegen_das_manifest_ohne_server() {
    // Server läuft nicht – das Manifest genügt
    let q = tempfile::tempdir().unwrap();
    let toml = format!(
        "id = \"t\"\nname = \"T\"\nversion = 1\n[[stufen]]\nid = \"server\"\nname = \"S\"\nquelle = {{ typ = \"lokal\", pfad = \"{}\" }}\nziel = {{ typ = \"ftps\", host = \"gibt-es-nicht.invalid\", pfad = \"/x\", benutzer = \"b\", konto_ref = \"k\" }}\n",
        q.path().display()
    );
    let strecke = strecke_aus_toml(&toml, Path::new("t.toml")).unwrap();
    datei(q.path(), "zeit-gleich.txt", "aaa");
    datei(q.path(), "nur-zeit-anders.txt", "bbb");
    datei(q.path(), "inhalt-anders.txt", "ccc");
    datei(q.path(), "ungeprueft.txt", "ddd");
    let manifest = [
        m("zeit-gleich.txt", 3, Some("xxx"), Some(T)), // gleiche Zeit: nicht gelesen, gilt als gleich
        m("nur-zeit-anders.txt", 3, Some("bbb"), Some(T - 1)),
        m("inhalt-anders.txt", 3, Some("CCC"), Some(T - 1)),
        m("ungeprueft.txt", 3, None, None),
        m("ordner", 0, None, None),
        m(".git", 0, None, None), // ignoriert – samt Inhalt
        m(".git/HEAD", 4, Some("head"), None),
    ];
    let v = stufe_vergleichen_entfernt(&strecke, "server", &GlobalConfig::default(), EntfernterStand::Manifest(&manifest), &AtomicBool::new(false), &mut |_| {})
        .unwrap();
    assert_eq!(
        status(&v),
        [
            (".git".to_string(), DiffStatus::Ignoriert),
            ("inhalt-anders.txt".to_string(), DiffStatus::Geaendert),
            ("nur-zeit-anders.txt".into(), DiffStatus::Gleich),
            ("ordner".into(), DiffStatus::Loeschen),
            ("ungeprueft.txt".into(), DiffStatus::Gleich),
            ("zeit-gleich.txt".into(), DiffStatus::Gleich),
        ]
    );
    assert_eq!(v.vergleich.vergleich.eintraege[4].warnungen, vec![Warnung::InhaltNichtGeprueft]);
    assert!(v.neues_manifest.is_none());
}

#[test]
fn falsche_art_des_ziels_und_fehlende_quelle() {
    let f = fall(Aufbau::default());
    let e = stufe_vergleichen(&f.strecke, "server", &GlobalConfig::default(), &AtomicBool::new(false), &mut |_| {}).unwrap_err();
    assert_eq!(e.kategorie(), "nicht_unterstuetzt");

    let mut lokal = f.strecke.clone();
    lokal.stufen[0].ziel = Endpunkt::Lokal { pfad: f.z.clone() };
    let e = stufe_vergleichen_entfernt(&lokal, "server", &GlobalConfig::default(), EntfernterStand::Manifest(&[]), &AtomicBool::new(false), &mut |_| {})
        .unwrap_err();
    assert_eq!(e.kategorie(), "nicht_unterstuetzt");

    let mut weg = f.strecke.clone();
    let quelle = f.q.join("unter");
    fs::create_dir(&quelle).unwrap();
    weg.stufen[0].quelle = stellwerk_core::model::Quelle::Lokal { pfad: quelle.clone() };
    fs::remove_dir(&quelle).unwrap();
    let e = stufe_vergleichen_entfernt(&weg, "server", &GlobalConfig::default(), EntfernterStand::Manifest(&[]), &AtomicBool::new(false), &mut |_| {})
        .unwrap_err();
    assert!(matches!(e.kategorie(), "kein_verzeichnis" | "validierung"), "{e}");
}

#[test]
fn manifest_abgleichen_behaelt_hash_nur_bei_gleicher_groesse_und_art() {
    let alt = [m("gleich.txt", 3, Some("abc"), Some(T)), m("groesser.txt", 3, Some("abc"), Some(T)), m("jetzt-ordner", 1, Some("x"), None), m("weg.txt", 1, Some("w"), None)];
    let f = fall(Aufbau::default());
    datei(&f.z, "gleich.txt", "xyz");
    datei(&f.z, "groesser.txt", "abcd");
    datei(&f.z, "jetzt-ordner/a", "a");
    let v = f.vergleichen(EntfernterStand::Server { ftps: &f.ftps(), manifest: &alt }).unwrap();
    let neu = v.neues_manifest.unwrap();
    assert_eq!(neu[0], alt[0], "gleiche Größe: Hash und Zeit bleiben");
    assert_eq!((neu[1].groesse, neu[1].hash, neu[1].lauf_id.as_deref()), (4, None, None));
    assert_eq!((neu[2].pfad.as_str(), neu[2].ist_ordner, neu[2].hash), ("jetzt-ordner", true, None));
    assert!(!neu.iter().any(|e| e.pfad.as_str() == "weg.txt"), "was der Server nicht hat, fällt heraus");
    // Ohne Server: leeres Ergebnis
    assert!(manifest_abgleichen(&alt, &ScanErgebnis::default()).is_empty());
}

// --- Inhalte prüfen (v0.5) ---------------------------------------------------

#[test]
fn inhalte_hashen_laedt_herunter_und_verzeiht_einzelne_fehler() {
    let f = fall(Aufbau::default());
    datei(&f.z, "a.txt", "abc");
    datei(&f.z, "web/b.css", "body{}");
    let pfade = [RelPath::new("a.txt").unwrap(), RelPath::new("fehlt.txt").unwrap(), RelPath::new("web/b.css").unwrap()];
    let mut stand = Vec::new();
    let erg = f.ftps().inhalte_hashen(&pfade, &AtomicBool::new(false), &mut |n| stand.push(n)).unwrap();
    assert_eq!(stand, [1, 2, 3]);
    let a = erg[0].1.as_ref().unwrap();
    assert_eq!((a.bytes, a.hash), (3, Hash(*blake3::hash(b"abc").as_bytes())));
    assert!(erg[1].1.as_ref().unwrap_err().contains("550"), "{:?}", erg[1]);
    assert_eq!(erg[2].1.as_ref().unwrap().hash, Hash(*blake3::hash(b"body{}").as_bytes()), "nach dem Fehler weiter");
    assert_eq!(fs::read_to_string(f.z.join("a.txt")).unwrap(), "abc", "am Server unverändert");

    // Abbruch: nichts wird mehr geprüft
    let e = f.ftps().inhalte_hashen(&pfade, &AtomicBool::new(true), &mut |_| {}).unwrap_err();
    assert!(matches!(e, StellwerkError::Abbruch));
}
