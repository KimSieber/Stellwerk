//! Modus „Umschalten“ (v0.5): Hochladen als Schattendatei → Umschalten →
//! Übernehmen oder Rollback; Abbrechen vor dem Umschalten. Lokal und gegen den
//! FTPS-Testserver.

#![cfg(unix)]

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use stellwerk_core::ansicht::{stufe_vergleichen, VergleichsSitzung};
use stellwerk_core::config::{strecke_aus_toml, GlobalConfig};
use stellwerk_core::endpoint::{Ftps, Lokal, ZielEndpunkt, ZielVerbindung};
use stellwerk_core::exec::umschalten::{
    abbrechen, backup_pfad, hochladen, schatten_pfad, uebernehmen, umschalten, zurueckrollen, PhasenErgebnis, SchattenZiel,
};
use stellwerk_core::exec::{LaufErgebnis, Optionen};
use stellwerk_core::model::{Dateimodus, FtpsEndpunkt, Rechte, RelPath, Tls};
use stellwerk_core::plan::{plan_erstellen, OpArt, Plan};
use stellwerk_core::store::JournalStatus;
use stellwerk_core::StellwerkError;
use stellwerk_testserver::{starten, Aufbau, TestServer, BENUTZER, PASSWORT};

const LAUF: &str = "u1";

struct Ordner {
    _tmp: tempfile::TempDir,
    q: PathBuf,
    z: PathBuf,
}

fn ordner() -> Ordner {
    let tmp = tempfile::tempdir().unwrap();
    let [q, z] = ["quelle", "ziel"].map(|n| tmp.path().join(n));
    for d in [&q, &z] {
        fs::create_dir_all(d).unwrap();
    }
    Ordner { _tmp: tmp, q, z }
}

fn datei(wurzel: &Path, rel: &str, inhalt: &str) {
    let p = wurzel.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, inhalt).unwrap();
}

fn p(s: &str) -> RelPath {
    RelPath::new(s).unwrap()
}

/// Plan wie bei einer lokalen Stufe Quelle → Ziel (auch für den Ordner des Testservers).
fn plan(q: &Path, z: &Path, rechte: &Rechte) -> Plan {
    let toml = format!(
        "id = \"t\"\nname = \"T\"\nversion = 1\n[[stufen]]\nid = \"s\"\nname = \"S\"\nquelle = {{ typ = \"lokal\", pfad = \"{}\" }}\nziel = {{ typ = \"lokal\", pfad = \"{}\" }}\n",
        q.display(),
        z.display()
    );
    let s = strecke_aus_toml(&toml, Path::new("t.toml")).unwrap();
    let v = stufe_vergleichen(&s, "s", &GlobalConfig::default(), &AtomicBool::new(false), &mut |_| {}).unwrap();
    plan_erstellen(VergleichsSitzung::neu(v.vergleich).eintraege(), rechte).unwrap()
}

/// Inhalte aller Dateien (und Ordner als `None`) unter `wurzel`.
fn inhalte(wurzel: &Path) -> BTreeMap<String, Option<String>> {
    fn walk(d: &Path, wurzel: &Path, m: &mut BTreeMap<String, Option<String>>) {
        for e in fs::read_dir(d).unwrap() {
            let p = e.unwrap().path();
            let rel = p.strip_prefix(wurzel).unwrap().display().to_string();
            if p.is_dir() {
                m.insert(rel, None);
                walk(&p, wurzel, m);
            } else {
                m.insert(rel, Some(fs::read_to_string(&p).unwrap()));
            }
        }
    }
    let mut m = BTreeMap::new();
    walk(wurzel, wurzel, &mut m);
    m
}

fn webseite(q: &Path, z: &Path) {
    datei(q, "index.php", "neu!");
    datei(z, "index.php", "alt");
    datei(q, "css/site.css", "c");
    datei(z, "css/site.css", "c");
    datei(q, "bilder/neu/a.png", "aa");
    datei(z, "alt/unter/x.txt", "x");
    datei(z, "weg.txt", "w");
}

fn opt() -> Optionen {
    Optionen { worker: 2, bei_fehler_stoppen: false, fortschritt_abstand: Duration::ZERO }
}

/// Phase 1 mit Journal (Plan-Nummer, Status).
fn phase1(plan: &Plan, q: &Path, ziel: &dyn ZielEndpunkt) -> (LaufErgebnis, Vec<(usize, JournalStatus)>) {
    phase1_mit(plan, q, ziel, 2)
}

/// Gegen den Testserver mit einer Verbindung: Gleichzeitige Sitzungen von libunftp
/// vertragen sich unter Linux nicht immer (siehe stellwerk-testserver).
fn phase1_mit(plan: &Plan, q: &Path, ziel: &dyn ZielEndpunkt, worker: usize) -> (LaufErgebnis, Vec<(usize, JournalStatus)>) {
    let mut j = Vec::new();
    let erg = hochladen(plan, &Lokal::new(q), ziel, LAUF, &Optionen { worker, ..opt() }, &AtomicBool::new(false), &mut |o, st, _, _, _| {
        j.push((o.nr, st));
        Ok(())
    }, &mut |_| {});
    (erg, j)
}

type Phase = fn(&Plan, &dyn ZielEndpunkt, &str, &mut stellwerk_core::exec::umschalten::PhaseMelden) -> stellwerk_core::Result<PhasenErgebnis>;

/// Phase 2/3 mit Journal (Plan-Nummer, Status, Meldung).
fn lauf(f: Phase, plan: &Plan, ziel: &dyn ZielEndpunkt) -> (PhasenErgebnis, Vec<(usize, JournalStatus, Option<String>)>) {
    let mut j = Vec::new();
    let erg = f(plan, ziel, LAUF, &mut |o, st, m| {
        j.push((o.nr, st, m.map(String::from)));
        Ok(())
    })
    .unwrap();
    (erg, j)
}

fn ohne_hilfsdateien(z: &Path) {
    let rest: Vec<_> = inhalte(z).into_keys().filter(|k| k.contains(".dpl-") || k.contains(".bak-") || k.contains(".stw-")).collect();
    assert!(rest.is_empty(), "{rest:?}");
}

// --- Lokal --------------------------------------------------------------------

#[test]
fn namen_der_hilfsdateien() {
    assert_eq!(schatten_pfad(&p("web/index.php"), "mw5"), p("web/index.php.dpl-mw5"));
    assert_eq!(backup_pfad(&p("index.php"), "mw5"), p("index.php.bak-mw5"));
}

#[test]
fn hochladen_umschalten_uebernehmen() {
    let o = ordner();
    webseite(&o.q, &o.z);
    let vorher = inhalte(&o.z);
    let plan = plan(&o.q, &o.z, &Rechte::NichtSetzen);
    let ziel = Lokal::new(&o.z);

    // Phase 1: Live unverändert, Schattendateien und neue Ordner da, nichts gelöscht
    let (erg, j) = phase1(&plan, &o.q, &ziel);
    assert!(erg.vollstaendig(), "{erg:?}");
    let loeschungen = plan.operationen.iter().filter(|o| matches!(o.art, OpArt::DateiLoeschen | OpArt::OrdnerLoeschen)).count();
    assert_eq!(j.len(), plan.operationen.len() - loeschungen, "Löschungen kommen in Phase 1 nicht vor");
    assert!(j.iter().all(|(nr, st)| *st == JournalStatus::Erledigt && !matches!(plan.operationen[*nr].art, OpArt::DateiLoeschen | OpArt::OrdnerLoeschen)));
    let nach1 = inhalte(&o.z);
    for (k, v) in &vorher {
        assert_eq!(nach1.get(k), Some(v), "{k} unverändert");
    }
    assert_eq!(nach1["index.php.dpl-u1"].as_deref(), Some("neu!"));
    assert_eq!(nach1["bilder/neu/a.png.dpl-u1"].as_deref(), Some("aa"));
    assert!(!nach1.contains_key("bilder/neu/a.png"));

    // Phase 2: umgeschaltet, Sicherungen da
    let (erg, j) = lauf(umschalten, &plan, &ziel);
    assert!(erg.vollstaendig(), "{erg:?}");
    assert_eq!(erg.erledigt.len(), 4, "überschreiben, neu, 2 Dateien löschen: {j:?}");
    let nach2 = inhalte(&o.z);
    assert_eq!(nach2["index.php"].as_deref(), Some("neu!"));
    assert_eq!(nach2["index.php.bak-u1"].as_deref(), Some("alt"));
    assert_eq!(nach2["weg.txt.bak-u1"].as_deref(), Some("w"));
    assert!(!nach2.contains_key("weg.txt"));
    assert!(nach2.contains_key("alt/unter"), "Ordner bleiben bis zum Übernehmen");

    // Phase 3: übernehmen – Ziel entspricht der Quelle
    let (erg, _) = lauf(uebernehmen, &plan, &ziel);
    assert!(erg.vollstaendig(), "{erg:?}");
    assert_eq!(inhalte(&o.z), inhalte(&o.q));
    // Noch einmal: alles schon geschehen
    assert!(lauf(uebernehmen, &plan, &ziel).0.vollstaendig());
    assert!(lauf(umschalten, &plan, &ziel).0.vollstaendig(), "Umschalten ist wiederholbar");
    assert_eq!(inhalte(&o.z), inhalte(&o.q));
}

#[test]
fn rollback_stellt_den_alten_stand_wieder_her() {
    let o = ordner();
    webseite(&o.q, &o.z);
    let vorher = inhalte(&o.z);
    let plan = plan(&o.q, &o.z, &Rechte::NichtSetzen);
    let ziel = Lokal::new(&o.z);
    assert!(phase1(&plan, &o.q, &ziel).0.vollstaendig());
    assert!(lauf(umschalten, &plan, &ziel).0.vollstaendig());
    let (erg, j) = lauf(zurueckrollen, &plan, &ziel);
    assert!(erg.vollstaendig(), "{erg:?}");
    assert!(j.windows(2).all(|w| w[0].0 > w[1].0), "rückwärts");
    assert_eq!(inhalte(&o.z), vorher);
}

#[test]
fn umschalten_stoppt_beim_ersten_fehler_rollback_danach() {
    let o = ordner();
    webseite(&o.q, &o.z);
    datei(&o.q, "zz-neu.txt", "n");
    let vorher = inhalte(&o.z);
    let plan = plan(&o.q, &o.z, &Rechte::NichtSetzen);
    let ziel = Lokal::new(&o.z);
    assert!(phase1(&plan, &o.q, &ziel).0.vollstaendig());
    // Jemand legt eine neue Datei inzwischen selbst an – Umschalten darf sie nicht ersetzen
    datei(&o.z, "bilder/neu/a.png", "fremd");
    let (erg, j) = lauf(umschalten, &plan, &ziel);
    assert!(erg.gestoppt && !erg.vollstaendig());
    assert_eq!(erg.fehler.len(), 1);
    assert_eq!(erg.fehler[0].1, p("bilder/neu/a.png"));
    assert!(erg.fehler[0].2.contains("inzwischen vorhanden"), "{erg:?}");
    assert_eq!(j.last().unwrap().1, JournalStatus::Fehler);
    assert!(!j.iter().any(|(nr, _, _)| plan.operationen[*nr].pfad == p("weg.txt")), "nach dem Fehler nichts mehr");

    // Rollback: zurück zum alten Stand – die fremde Datei bleibt unberührt, ihre
    // Ordner bleiben deshalb auch (nicht leer)
    let (erg, _) = lauf(zurueckrollen, &plan, &ziel);
    assert_eq!(erg.fehler.len(), 2, "bilder/neu und bilder sind nicht leer: {erg:?}");
    let mut erwartet = vorher;
    erwartet.insert("bilder".into(), None);
    erwartet.insert("bilder/neu".into(), None);
    erwartet.insert("bilder/neu/a.png".into(), Some("fremd".into()));
    assert_eq!(inhalte(&o.z), erwartet);
    ohne_hilfsdateien(&o.z);
}

#[test]
fn abbrechen_vor_dem_umschalten() {
    let o = ordner();
    webseite(&o.q, &o.z);
    let vorher = inhalte(&o.z);
    let plan = plan(&o.q, &o.z, &Rechte::NichtSetzen);
    let ziel = Lokal::new(&o.z);
    assert!(phase1(&plan, &o.q, &ziel).0.vollstaendig());
    let (erg, _) = lauf(abbrechen, &plan, &ziel);
    assert!(erg.vollstaendig(), "{erg:?}");
    assert_eq!(inhalte(&o.z), vorher);
}

#[test]
fn rechte_gelten_der_schattendatei_und_kommen_mit() {
    let o = ordner();
    datei(&o.q, "web/a.sh", "a");
    datei(&o.q, "b.txt", "neuer Inhalt");
    datei(&o.z, "b.txt", "alt");
    let r = Rechte::Fest { ordner: Dateimodus::new(0o750), dateien: Dateimodus::new(0o640) };
    let plan = plan(&o.q, &o.z, &r);
    let ziel = Lokal::new(&o.z);
    assert!(phase1(&plan, &o.q, &ziel).0.vollstaendig());
    let modus = |rel: &str| fs::metadata(o.z.join(rel)).unwrap().permissions().mode() & 0o777;
    assert_eq!([modus("web"), modus("web/a.sh.dpl-u1"), modus("b.txt.dpl-u1")], [0o750, 0o640, 0o640]);
    assert!(lauf(umschalten, &plan, &ziel).0.vollstaendig());
    assert_eq!([modus("web/a.sh"), modus("b.txt")], [0o640, 0o640]);
}

#[test]
fn fehler_beim_hochladen_tragen_die_nummer_des_plans() {
    let o = ordner();
    datei(&o.q, "a.txt", "a");
    datei(&o.z, "weg.txt", "w");
    datei(&o.q, "z.txt", "z");
    let plan = plan(&o.q, &o.z, &Rechte::NichtSetzen);
    fs::remove_file(o.q.join("z.txt")).unwrap();
    let (erg, _) = phase1(&plan, &o.q, &Lokal::new(&o.z));
    assert_eq!(erg.fehler.len(), 1);
    let nr = erg.fehler[0].nr;
    assert_eq!(plan.operationen[nr].pfad, p("z.txt"), "{erg:?}");
}

/// Ziel, dessen Verbindung scheitert.
struct KeinZiel;
impl ZielEndpunkt for KeinZiel {
    fn verbinden(&self) -> stellwerk_core::Result<Box<dyn ZielVerbindung + '_>> {
        Err(StellwerkError::Anmeldung("abgelehnt".into()))
    }
}

/// Ziel, dessen erste Verbindungen scheitern.
struct WackelndesZiel {
    innen: Lokal,
    versuche: std::sync::atomic::AtomicUsize,
    scheitern: usize,
}
impl ZielEndpunkt for WackelndesZiel {
    fn verbinden(&self) -> stellwerk_core::Result<Box<dyn ZielVerbindung + '_>> {
        if self.versuche.fetch_add(1, std::sync::atomic::Ordering::SeqCst) < self.scheitern {
            return Err(StellwerkError::Verbindung("kurz weg".into()));
        }
        self.innen.verbinden()
    }
}

#[test]
fn verbindung_wird_vor_dem_umschalten_erneut_versucht() {
    let o = ordner();
    datei(&o.q, "neu.txt", "n");
    let plan = plan(&o.q, &o.z, &Rechte::NichtSetzen);
    assert!(phase1(&plan, &o.q, &Lokal::new(&o.z)).0.vollstaendig());
    let ziel = WackelndesZiel { innen: Lokal::new(&o.z), versuche: Default::default(), scheitern: 2 };
    assert!(lauf(umschalten, &plan, &ziel).0.vollstaendig());
    assert_eq!(ziel.versuche.load(std::sync::atomic::Ordering::SeqCst), 3);
    // Andere Fehler und der letzte Versuch: kein weiterer
    let ziel = WackelndesZiel { innen: Lokal::new(&o.z), versuche: Default::default(), scheitern: 9 };
    assert_eq!(umschalten(&plan, &ziel, LAUF, &mut |_, _, _| Ok(())).unwrap_err().kategorie(), "verbindung");
    assert_eq!(ziel.versuche.load(std::sync::atomic::Ordering::SeqCst), stellwerk_core::exec::umschalten::VERBINDEN_VERSUCHE);
}

#[test]
fn phasen_ohne_arbeit_ohne_verbindung_und_mit_journalfehler() {
    let o = ordner();
    datei(&o.q, "neu.txt", "n");
    let plan = plan(&o.q, &o.z, &Rechte::NichtSetzen);
    // Nur Neues: Übernehmen hat nichts zu tun und verbindet gar nicht
    assert_eq!(uebernehmen(&plan, &KeinZiel, LAUF, &mut |_, _, _| Ok(())).unwrap(), PhasenErgebnis::default());
    assert_eq!(umschalten(&plan, &KeinZiel, LAUF, &mut |_, _, _| Ok(())).unwrap_err().kategorie(), "anmeldung");
    let e = umschalten(&plan, &Lokal::new(&o.z), LAUF, &mut |_, _, _| Err(StellwerkError::Datenbank("voll".into()))).unwrap_err();
    assert_eq!(e.to_string(), "Datenbank: voll");
}

#[test]
fn schattenverbindung_wirkt_auf_die_schattendatei() {
    let o = ordner();
    datei(&o.z, "a.txt.dpl-u1", "s");
    datei(&o.z, "b.txt", "b");
    fs::create_dir(o.z.join("leer")).unwrap();
    let innen = Lokal::new(&o.z);
    let ziel = SchattenZiel { innen: &innen, dateien: HashSet::from([p("a.txt")]), lauf_id: LAUF };
    let mut v = ziel.verbinden().unwrap();
    assert!(v.ist_datei(&p("a.txt")).unwrap(), "fragt die Schattendatei");
    assert!(!v.ist_datei(&p("leer")).unwrap());
    v.umbenennen(&p("b.txt"), &p("c.txt"), false).unwrap();
    v.datei_loeschen(&p("a.txt")).unwrap();
    v.ordner_loeschen(&p("leer")).unwrap();
    assert_eq!(inhalte(&o.z), BTreeMap::from([("c.txt".to_string(), Some("b".to_string()))]));
    assert_eq!(v.hinweis(), None);
    assert!(KeinZiel.verbinden().is_err());
    assert!(SchattenZiel { innen: &KeinZiel, dateien: HashSet::new(), lauf_id: LAUF }.verbinden().is_err());
}

#[test]
fn lokales_umbenennen_ersetzt_nur_auf_wunsch() {
    let o = ordner();
    datei(&o.z, "a", "a");
    datei(&o.z, "b", "b");
    let ziel = Lokal::new(&o.z);
    let mut v = ziel.verbinden().unwrap();
    assert_eq!(v.umbenennen(&p("a"), &p("b"), false).unwrap_err().kategorie(), "vorhanden");
    v.umbenennen(&p("a"), &p("b"), true).unwrap();
    assert_eq!(inhalte(&o.z), BTreeMap::from([("b".to_string(), Some("a".to_string()))]));
    assert_eq!(v.umbenennen(&p("fehlt"), &p("c"), true).unwrap_err().kategorie(), "io");
}

// --- FTPS ---------------------------------------------------------------------

struct Server {
    q: Ordner,
    s: TestServer,
    z: PathBuf,
}

fn server(a: Aufbau) -> Server {
    let q = ordner();
    let s = starten(a);
    let z = s.ordner("httpdocs");
    Server { q, s, z }
}

impl Server {
    fn ftps(&self) -> Ftps {
        let z = FtpsEndpunkt {
            host: "127.0.0.1".into(),
            port: self.s.port,
            pfad: "/httpdocs".into(),
            benutzer: BENUTZER.into(),
            tls: Tls::Explizit,
            konto_ref: "k".into(),
            zertifikat: Some(self.s.fingerprint.clone()),
        };
        Ftps::new(&z, PASSWORT).unwrap()
    }
}

#[test]
fn ftps_alle_phasen_und_rollback_auch_wenn_der_server_nicht_ueberschreibt() {
    let sv = server(Aufbau { umbenennen_ueberschreibt_nicht: true, ..Default::default() });
    webseite(&sv.q.q, &sv.z);
    let vorher = inhalte(&sv.z);
    let plan = plan(&sv.q.q, &sv.z, &Rechte::NichtSetzen);
    let ftps = sv.ftps();
    assert!(phase1_mit(&plan, &sv.q.q, &ftps, 1).0.vollstaendig());
    assert_eq!(inhalte(&sv.z)["index.php"].as_deref(), Some("alt"), "Live unverändert");
    assert!(lauf(umschalten, &plan, &ftps).0.vollstaendig());
    assert_eq!(inhalte(&sv.z)["index.php"].as_deref(), Some("neu!"));
    // Rollback ersetzt die neue Datei durch die Sicherung – der Server lehnt das
    // Überschreiben ab, der Rückfall löscht vorher
    let (erg, _) = lauf(zurueckrollen, &plan, &ftps);
    assert!(erg.vollstaendig(), "{erg:?}");
    assert_eq!(inhalte(&sv.z), vorher);

    // Noch einmal bis zum Übernehmen
    assert!(phase1_mit(&plan, &sv.q.q, &ftps, 1).0.vollstaendig());
    assert!(lauf(umschalten, &plan, &ftps).0.vollstaendig());
    assert!(lauf(uebernehmen, &plan, &ftps).0.vollstaendig());
    assert_eq!(inhalte(&sv.z), inhalte(&sv.q.q));
}

#[test]
fn ftps_umschalten_ersetzt_keine_fremde_datei_abbrechen_raeumt_auf() {
    let sv = server(Aufbau::default());
    datei(&sv.q.q, "neu.txt", "n");
    let plan = plan(&sv.q.q, &sv.z, &Rechte::NichtSetzen);
    let ftps = sv.ftps();
    assert!(phase1_mit(&plan, &sv.q.q, &ftps, 1).0.vollstaendig());
    datei(&sv.z, "neu.txt", "fremd");
    let (erg, _) = lauf(umschalten, &plan, &ftps);
    assert!(erg.gestoppt);
    assert!(erg.fehler[0].2.contains("am Server inzwischen vorhanden"), "{erg:?}");
    fs::remove_file(sv.z.join("neu.txt")).unwrap();
    let (erg, _) = lauf(abbrechen, &plan, &ftps);
    assert!(erg.vollstaendig(), "{erg:?}");
    assert!(inhalte(&sv.z).is_empty(), "{:?}", inhalte(&sv.z));
}

#[test]
fn schreibweise_wechselt_alte_datei_weicht_vor_dem_umschalten() {
    let o = ordner();
    datei(&o.q, "Bild.jpg", "neu");
    datei(&o.z, "bild.jpg", "alt");
    let plan = plan(&o.q, &o.z, &Rechte::NichtSetzen);
    let ziel = Lokal::new(&o.z);
    assert!(phase1(&plan, &o.q, &ziel).0.vollstaendig());
    assert!(lauf(umschalten, &plan, &ziel).0.vollstaendig());
    assert!(lauf(uebernehmen, &plan, &ziel).0.vollstaendig());
    assert_eq!(inhalte(&o.z), inhalte(&o.q));
}

#[test]
fn fortsetzen_nach_halbem_tausch_und_uebernehmen_laeuft_bei_fehlern_weiter() {
    let o = ordner();
    webseite(&o.q, &o.z);
    let plan = plan(&o.q, &o.z, &Rechte::NichtSetzen);
    let ziel = Lokal::new(&o.z);
    assert!(phase1(&plan, &o.q, &ziel).0.vollstaendig());
    // Abbruch nach der ersten Umbenennung: Original schon Sicherung, Schattendatei noch da
    fs::rename(o.z.join("index.php"), o.z.join("index.php.bak-u1")).unwrap();
    assert!(lauf(umschalten, &plan, &ziel).0.vollstaendig());
    assert_eq!(inhalte(&o.z)["index.php"].as_deref(), Some("neu!"));
    // Inzwischen liegt etwas im zu löschenden Ordner: Fehler, der Rest wird trotzdem übernommen
    datei(&o.z, "alt/unter/neu.txt", "dazu");
    let (erg, _) = lauf(uebernehmen, &plan, &ziel);
    assert!(!erg.gestoppt);
    assert_eq!(erg.fehler.len(), 2, "alt/unter und alt: {erg:?}");
    assert!(!inhalte(&o.z).contains_key("index.php.bak-u1"));
    assert!(!inhalte(&o.z).contains_key("weg.txt.bak-u1"));
}
