//! Ausführung „Direkt“ lokal: Vergleich → Plan → Worker-Pool → erneuter Vergleich
//! (ARCHITEKTUR Kap. 8, Übergabe v0.3 Kap. 6).

#![cfg(unix)]

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use filetime::{set_file_mtime, FileTime};
use stellwerk_core::ansicht::{stufe_vergleichen, VergleichsSitzung};
use stellwerk_core::config::{strecke_aus_toml, GlobalConfig};
use stellwerk_core::diff::Zusammenfassung;
use stellwerk_core::exec::{
    direkt_ausfuehren, standard_worker, ExecFortschritt, LaufErgebnis, Optionen, MAX_WORKER,
};
use stellwerk_core::model::{Dateimodus, Rechte};
use stellwerk_core::plan::{plan_erstellen, OpArt, Plan};
use stellwerk_core::store::JournalStatus;
use stellwerk_core::StellwerkError;

const LAUF: &str = "l1";

// --- Testaufbau ------------------------------------------------------------

struct Aufbau {
    _tmp: tempfile::TempDir,
    q: PathBuf,
    z: PathBuf,
}

fn aufbau() -> Aufbau {
    let tmp = tempfile::tempdir().unwrap();
    let [q, z] = ["quelle", "ziel"].map(|n| tmp.path().join(n));
    for d in [&q, &z] {
        fs::create_dir_all(d).unwrap();
    }
    Aufbau { _tmp: tmp, q, z }
}

impl Aufbau {
    fn vergleich_mit(&self, zusatz: &str) -> (VergleichsSitzung, Zusammenfassung) {
        let toml = format!(
            r#"
id = "test"
name = "Test"
version = 1

[[stufen]]
id = "spiegel"
name = "Spiegel"
quelle = {{ typ = "lokal", pfad = "{q}" }}
ziel = {{ typ = "lokal", pfad = "{z}" }}
{zusatz}
"#,
            q = self.q.display(),
            z = self.z.display()
        );
        let s = strecke_aus_toml(&toml, Path::new("test.toml")).unwrap();
        let v = stufe_vergleichen(&s, "spiegel", &GlobalConfig::default(), &AtomicBool::new(false), &mut |_| {})
            .unwrap();
        let z = v.vergleich.zusammenfassung.clone();
        (VergleichsSitzung::neu(v.vergleich), z)
    }

    fn plan_mit(&self, zusatz: &str, rechte: &Rechte) -> Plan {
        plan_erstellen(self.vergleich_mit(zusatz).0.eintraege(), rechte).unwrap()
    }

    fn plan(&self) -> Plan {
        self.plan_mit("", &Rechte::NichtSetzen)
    }

    /// Nach einem vollständigen Lauf: nichts mehr zu tun.
    fn alles_gleich(&self, zusatz: &str) {
        let z = self.vergleich_mit(zusatz).1;
        assert_eq!((z.neu, z.geaendert, z.loeschen, z.konflikt), (0, 0, 0, 0), "{z:?}");
    }
}

fn datei(wurzel: &Path, rel: &str, inhalt: &str) {
    let p = wurzel.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, inhalt).unwrap();
    set_file_mtime(&p, FileTime::from_unix_time(1_700_000_000, 123_456_789)).unwrap();
}

fn opt(worker: usize) -> Optionen {
    Optionen { worker, bei_fehler_stoppen: false, fortschritt_abstand: Duration::ZERO }
}

struct Lauf {
    erg: LaufErgebnis,
    /// Journal-Meldungen in Reihenfolge: (nr, Status, Meldung)
    journal: Vec<(usize, JournalStatus, Option<String>)>,
    fortschritt: Vec<ExecFortschritt>,
}

impl Lauf {
    fn status(&self) -> BTreeMap<usize, (JournalStatus, Option<String>)> {
        self.journal.iter().map(|(n, s, m)| (*n, (*s, m.clone()))).collect()
    }
}

fn laufen(a: &Aufbau, plan: &Plan, o: &Optionen) -> Lauf {
    laufen_mit(a, plan, o, &AtomicBool::new(false), &mut |_, _| Ok(()))
}

fn laufen_mit(
    a: &Aufbau,
    plan: &Plan,
    o: &Optionen,
    abbruch: &AtomicBool,
    zusatz: &mut dyn FnMut(usize, JournalStatus) -> stellwerk_core::Result<()>,
) -> Lauf {
    let mut journal = Vec::new();
    let mut fortschritt = Vec::new();
    let erg = direkt_ausfuehren(
        plan,
        &a.q,
        &a.z,
        LAUF,
        o,
        abbruch,
        &mut |op, st, m| {
            journal.push((op.nr, st, m.map(String::from)));
            zusatz(op.nr, st)
        },
        &mut |f| fortschritt.push(f),
    );
    // Jede Operation genau einmal im Journal
    let mut nrn: Vec<usize> = journal.iter().map(|j| j.0).collect();
    nrn.sort_unstable();
    assert_eq!(nrn, (0..plan.operationen.len()).collect::<Vec<_>>());
    Lauf { erg, journal, fortschritt }
}

/// Inhalt, Änderungszeit und Rechte aller Einträge unter `wurzel`.
fn abbild(wurzel: &Path) -> BTreeMap<String, (Option<Vec<u8>>, i64, u32)> {
    walk(wurzel)
        .into_iter()
        .map(|p| {
            let m = fs::symlink_metadata(&p).unwrap();
            let inhalt = m.is_file().then(|| fs::read(&p).unwrap());
            let mtime = if m.is_file() { FileTime::from_last_modification_time(&m).unix_seconds() } else { 0 };
            (p.strip_prefix(wurzel).unwrap().display().to_string(), (inhalt, mtime, m.permissions().mode() & 0o777))
        })
        .collect()
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

fn inhalte(wurzel: &Path) -> BTreeMap<String, Option<Vec<u8>>> {
    abbild(wurzel).into_iter().map(|(k, (i, _, _))| (k, i)).collect()
}

fn webseite(a: &Aufbau) {
    datei(&a.q, "index.php", "neu!");
    datei(&a.z, "index.php", "alt");
    datei(&a.q, "css/site.css", "c");
    datei(&a.z, "css/site.css", "c");
    datei(&a.q, "css/neu.css", "nnn");
    datei(&a.q, "bilder/neu/a.png", "aa");
    datei(&a.q, "bilder/neu/b.png", "bbb");
    fs::create_dir_all(a.q.join("leer")).unwrap();
    datei(&a.z, "alt/unter/x.txt", "x");
    datei(&a.z, "alt/y.txt", "y");
    datei(&a.z, "weg.txt", "w");
}

// --- Vollständiger Lauf ------------------------------------------------------

#[test]
fn ziel_entspricht_danach_der_quelle_mit_jeder_workerzahl() {
    for worker in [1, 3, MAX_WORKER] {
        let a = aufbau();
        webseite(&a);
        let quelle_vorher = abbild(&a.q);
        let plan = a.plan();
        let l = laufen(&a, &plan, &opt(worker));

        assert!(l.erg.vollstaendig(), "{:?}", l.erg);
        assert_eq!(l.erg.erledigt, plan.operationen.len());
        assert_eq!(l.erg.bytes, 4 + 3 + 2 + 3);
        assert!(l.journal.iter().all(|(_, s, m)| *s == JournalStatus::Erledigt && m.is_none()));

        assert_eq!(abbild(&a.q), quelle_vorher, "die Quelle bleibt unverändert");
        let mut erwartet = abbild(&a.q);
        let mut ziel = abbild(&a.z);
        // Rechte neuer Dateien = Quelle, Inhalt und Änderungszeit gleich, keine Temporärdateien
        erwartet.retain(|k, _| !k.is_empty());
        ziel.retain(|k, _| !k.is_empty());
        assert_eq!(ziel, erwartet, "Worker: {worker}");
        a.alles_gleich("");

        let erster = l.fortschritt.first().unwrap();
        let letzter = l.fortschritt.last().unwrap();
        assert_eq!((erster.fertig, erster.bytes), (0, 0));
        assert_eq!(
            *letzter,
            ExecFortschritt { fertig: plan.operationen.len(), gesamt: plan.operationen.len(), bytes: 12, bytes_gesamt: 12, fehler: 0 }
        );
    }
}

#[test]
fn leerer_plan() {
    let a = aufbau();
    let l = laufen(&a, &a.plan(), &opt(4));
    assert!(l.erg.vollstaendig());
    assert_eq!(l.erg, LaufErgebnis::default());
    assert_eq!(l.fortschritt.len(), 2);
}

#[test]
fn reihenfolge_haelt_abhaengigkeiten_ein() {
    let a = aufbau();
    webseite(&a);
    datei(&a.q, "Bild.jpg", "neu");
    datei(&a.z, "bild.jpg", "alt");
    let plan = a.plan();
    let l = laufen(&a, &plan, &opt(MAX_WORKER));
    assert!(l.erg.vollstaendig(), "{:?}", l.erg);
    let pos: BTreeMap<usize, usize> = l.journal.iter().enumerate().map(|(i, (nr, _, _))| (*nr, i)).collect();
    for o in &plan.operationen {
        for v in &o.nach {
            assert!(pos[v] < pos[&o.nr], "{} vor {}", plan.operationen[*v].pfad, o.pfad);
        }
    }
    // Namen aus dem Ordner lesen – auf macOS (Groß/Klein egal) fände `exists("bild.jpg")`
    // auch „Bild.jpg“. Am Ziel steht danach genau die Schreibweise der Quelle.
    let namen: Vec<String> = fs::read_dir(&a.z).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert!(namen.contains(&"Bild.jpg".to_string()), "{namen:?}");
    assert!(!namen.contains(&"bild.jpg".to_string()), "{namen:?}");
    a.alles_gleich("");
}

#[test]
fn ersetzen_durch_schreibt_den_inhalt_der_ersatzdatei() {
    let a = aufbau();
    datei(&a.q, "config.php", "entwicklung");
    datei(&a.q, "config.server.php", "server!");
    datei(&a.z, "config.php", "alt");
    let regel = r#"regeln.eigene = [ { muster = "/config.php", typ = "ersetzen", quelle = "config.server.php" } ]"#;
    let plan = a.plan_mit(regel, &Rechte::NichtSetzen);
    assert!(laufen(&a, &plan, &opt(2)).erg.vollstaendig());
    assert_eq!(fs::read_to_string(a.z.join("config.php")).unwrap(), "server!");
    assert_eq!(fs::read_to_string(a.q.join("config.php")).unwrap(), "entwicklung");
}

// --- Rechte ------------------------------------------------------------------

fn modus(p: &Path) -> u32 {
    fs::metadata(p).unwrap().permissions().mode() & 0o777
}

#[test]
fn rechte_ohne_stufeneinstellung_neu_wie_quelle_ueberschrieben_wie_bisher() {
    let a = aufbau();
    datei(&a.q, "start.sh", "#!/bin/sh");
    fs::set_permissions(a.q.join("start.sh"), fs::Permissions::from_mode(0o750)).unwrap();
    datei(&a.q, "geheim.txt", "neu!");
    datei(&a.z, "geheim.txt", "alt");
    fs::set_permissions(a.z.join("geheim.txt"), fs::Permissions::from_mode(0o600)).unwrap();
    assert!(laufen(&a, &a.plan(), &opt(2)).erg.vollstaendig());
    assert_eq!(modus(&a.z.join("start.sh")), 0o750);
    assert_eq!(modus(&a.z.join("geheim.txt")), 0o600);
    assert_eq!(fs::read_to_string(a.z.join("geheim.txt")).unwrap(), "neu!");
}

#[test]
fn rechte_der_stufe_werden_gesetzt() {
    let a = aufbau();
    datei(&a.q, "web/sub/a.php", "a");
    datei(&a.q, "b.txt", "neu!");
    datei(&a.z, "b.txt", "alt");
    let r = Rechte::Fest { ordner: Dateimodus::new(0o750), dateien: Dateimodus::new(0o640) };
    let l = laufen(&a, &a.plan_mit("", &r), &opt(4));
    assert!(l.erg.vollstaendig());
    assert_eq!(
        [modus(&a.z.join("web")), modus(&a.z.join("web/sub")), modus(&a.z.join("web/sub/a.php")), modus(&a.z.join("b.txt"))],
        [0o750, 0o750, 0o640, 0o640]
    );
}

// --- Fehler je Datei ------------------------------------------------------------

#[test]
fn fehler_je_datei_lauf_geht_weiter_abhaengiges_wird_uebersprungen() {
    let a = aufbau();
    datei(&a.q, "verschwindet.txt", "v");
    datei(&a.q, "neu/a.txt", "a");
    datei(&a.q, "neu/b.txt", "b");
    datei(&a.q, "ok.txt", "ok");
    let plan = a.plan();
    // Nach dem Vergleich: Quelle verschwindet, und am Ziel liegt eine Datei, wo ein Ordner hin soll
    fs::remove_file(a.q.join("verschwindet.txt")).unwrap();
    datei(&a.z, "neu", "Datei statt Ordner");

    let l = laufen(&a, &plan, &opt(1));
    assert_eq!(l.erg.erledigt, 1);
    assert_eq!(l.erg.uebersprungen, 2);
    assert!(!l.erg.gestoppt && !l.erg.abgebrochen && !l.erg.vollstaendig());
    let fehler: Vec<(OpArt, String)> = l.erg.fehler.iter().map(|f| (f.art, f.pfad.to_string())).collect();
    assert_eq!(fehler, vec![(OpArt::OrdnerAnlegen, "neu".into()), (OpArt::Kopieren, "verschwindet.txt".into())]);
    assert!(l.erg.fehler[1].meldung.contains("verschwindet.txt"), "{}", l.erg.fehler[1].meldung);

    let st = l.status();
    let nr = |art, p: &str| plan.operationen.iter().find(|o| o.art == art && o.pfad.as_str() == p).unwrap().nr;
    assert_eq!(st[&nr(OpArt::Kopieren, "ok.txt")].0, JournalStatus::Erledigt);
    assert_eq!(st[&nr(OpArt::OrdnerAnlegen, "neu")].0, JournalStatus::Fehler);
    assert_eq!(
        st[&nr(OpArt::Kopieren, "neu/a.txt")],
        (JournalStatus::Uebersprungen, Some("Voraussetzung nicht erfüllt".into()))
    );
    assert_eq!(l.fortschritt.last().unwrap().fehler, 2);
    assert_eq!(l.fortschritt.last().unwrap().fertig, 3);
    assert!(fs::read_dir(&a.z).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().contains(".stw-")));
}

#[test]
fn bei_fehler_stoppen_beginnt_nichts_neues() {
    let a = aufbau();
    datei(&a.q, "a.txt", "a");
    datei(&a.q, "b.txt", "b");
    datei(&a.q, "c.txt", "c");
    let plan = a.plan();
    fs::remove_file(a.q.join("a.txt")).unwrap();
    let l = laufen(&a, &plan, &Optionen { bei_fehler_stoppen: true, ..opt(1) });
    assert!(l.erg.gestoppt);
    assert_eq!((l.erg.fehler.len(), l.erg.erledigt, l.erg.uebersprungen), (1, 0, 2));
    assert_eq!(l.status()[&1], (JournalStatus::Uebersprungen, Some("Lauf vorzeitig beendet".into())));
    assert!(!a.z.join("b.txt").exists());
}

#[test]
fn neue_datei_am_ziel_inzwischen_vorhanden_wird_nicht_ueberschrieben() {
    let a = aufbau();
    datei(&a.q, "a.txt", "aus der Quelle");
    let plan = a.plan();
    datei(&a.z, "a.txt", "inzwischen da");
    let l = laufen(&a, &plan, &opt(1));
    assert_eq!(l.erg.fehler[0].meldung.contains("inzwischen vorhanden, bitte neu vergleichen"), true);
    assert_eq!(fs::read_to_string(a.z.join("a.txt")).unwrap(), "inzwischen da");
}

#[test]
fn vorhandene_temporaerdatei_wird_nie_angefasst() {
    let a = aufbau();
    datei(&a.q, "a.txt", "a");
    datei(&a.z, ".a.txt.stw-l1", "fremd");
    let plan = a.plan_mit(r#"regeln.eigene = [ { muster = ".a.txt.stw-l1", typ = "schuetzen" } ]"#, &Rechte::NichtSetzen);
    let l = laufen(&a, &plan, &opt(1));
    assert_eq!(l.erg.fehler.len(), 1);
    assert_eq!(fs::read_to_string(a.z.join(".a.txt.stw-l1")).unwrap(), "fremd");
    assert!(!a.z.join("a.txt").exists());
}

#[test]
fn quelle_ist_inzwischen_ein_ordner() {
    let a = aufbau();
    datei(&a.q, "a.txt", "a");
    let plan = a.plan();
    fs::remove_file(a.q.join("a.txt")).unwrap();
    fs::create_dir(a.q.join("a.txt")).unwrap();
    let l = laufen(&a, &plan, &opt(1));
    assert!(l.erg.fehler[0].meldung.contains("ist keine Datei"), "{:?}", l.erg.fehler);
}

#[test]
fn umbenennen_scheitert_temporaerdatei_wird_entfernt() {
    let a = aufbau();
    datei(&a.q, "a.txt", "neu!");
    datei(&a.z, "a.txt", "alt");
    let plan = a.plan();
    // Ziel ist inzwischen ein nicht leerer Ordner – Umbenennen darüber scheitert
    fs::remove_file(a.z.join("a.txt")).unwrap();
    datei(&a.z, "a.txt/drin", "d");
    let l = laufen(&a, &plan, &opt(1));
    assert_eq!(l.erg.fehler.len(), 1);
    assert_eq!(inhalte(&a.z).keys().cloned().collect::<Vec<_>>(), vec!["a.txt", "a.txt/drin"]);
}

#[test]
fn loeschen_ist_vorsichtig() {
    let a = aufbau();
    datei(&a.z, "schon-weg.txt", "s");
    datei(&a.z, "wird-ordner.txt", "w");
    datei(&a.z, "ordner-weg/x", "x");
    datei(&a.z, "ordner-voll/y", "y");
    datei(&a.z, "ordner-da", "o");
    let plan = a.plan();
    fs::remove_file(a.z.join("schon-weg.txt")).unwrap();
    fs::remove_file(a.z.join("wird-ordner.txt")).unwrap();
    fs::create_dir(a.z.join("wird-ordner.txt")).unwrap();
    fs::remove_dir_all(a.z.join("ordner-weg")).unwrap();
    datei(&a.z, "ordner-voll/neu-dazugekommen", "n");

    let l = laufen(&a, &plan, &opt(2));
    let mut fehler: Vec<(String, String)> = l.erg.fehler.iter().map(|f| (f.pfad.to_string(), f.meldung.clone())).collect();
    fehler.sort();
    assert_eq!(fehler.iter().map(|f| f.0.as_str()).collect::<Vec<_>>(), vec!["ordner-voll", "wird-ordner.txt"]);
    assert!(fehler[1].1.contains("inzwischen ein Ordner"), "{fehler:?}");
    assert!(a.z.join("ordner-voll/neu-dazugekommen").is_file(), "Nicht Geplantes bleibt");
    assert!(a.z.join("wird-ordner.txt").is_dir());
    assert!(!a.z.join("ordner-da").exists());
}

#[test]
fn ordner_anlegen_wenn_er_inzwischen_existiert() {
    let a = aufbau();
    datei(&a.q, "neu/a.txt", "a");
    let plan = a.plan();
    fs::create_dir(a.z.join("neu")).unwrap();
    assert!(laufen(&a, &plan, &opt(1)).erg.vollstaendig());
}

// --- Abbruch -------------------------------------------------------------------

#[test]
fn abbruch_vor_dem_start() {
    let a = aufbau();
    webseite(&a);
    let plan = a.plan();
    let vorher = inhalte(&a.z);
    let l = laufen_mit(&a, &plan, &opt(4), &AtomicBool::new(true), &mut |_, _| Ok(()));
    assert!(l.erg.abgebrochen);
    assert_eq!((l.erg.erledigt, l.erg.uebersprungen), (0, plan.operationen.len()));
    assert!(l.journal.iter().all(|(_, s, m)| *s == JournalStatus::Uebersprungen && m.as_deref() == Some("Lauf abgebrochen")
        || m.as_deref() == Some("Voraussetzung nicht erfüllt")));
    assert_eq!(inhalte(&a.z), vorher);
}

#[test]
fn abbruch_nach_der_ersten_operation() {
    let a = aufbau();
    datei(&a.q, "a.txt", "a");
    datei(&a.q, "b.txt", "b");
    let plan = a.plan();
    let abbruch = AtomicBool::new(false);
    let l = laufen_mit(&a, &plan, &opt(1), &abbruch, &mut |_, _| {
        abbruch.store(true, Ordering::Relaxed);
        Ok(())
    });
    assert!(l.erg.abgebrochen);
    assert_eq!((l.erg.erledigt, l.erg.uebersprungen), (1, 1));
    assert_eq!(l.status()[&1], (JournalStatus::Uebersprungen, Some("Lauf abgebrochen".into())));
}

#[test]
fn abbruch_waehrend_einer_kopie_raeumt_auf() {
    let a = aufbau();
    // Große Datei, die blockweise kopiert wird; abgebrochen wird, sobald die
    // Temporärdatei am Ziel erscheint – also mitten in der Kopie.
    let block = vec![7u8; 1 << 20];
    let mut f = fs::File::create(a.q.join("gross.bin")).unwrap();
    for _ in 0..64 {
        std::io::Write::write_all(&mut f, &block).unwrap();
    }
    drop(f);
    let plan = a.plan();
    let abbruch = AtomicBool::new(false);
    let tmp = a.z.join(".gross.bin.stw-l1");
    let l = std::thread::scope(|s| {
        s.spawn(|| {
            while !tmp.exists() {
                std::thread::yield_now();
            }
            abbruch.store(true, Ordering::Relaxed);
        });
        laufen_mit(&a, &plan, &opt(1), &abbruch, &mut |_, _| Ok(()))
    });
    assert!(l.erg.abgebrochen);
    assert_eq!(l.status()[&0], (JournalStatus::Uebersprungen, Some("Lauf abgebrochen".into())));
    assert_eq!(inhalte(&a.z).len(), 0, "weder Datei noch Temporärdatei am Ziel");
}

// --- Journal und Fortschritt -------------------------------------------------------

#[test]
fn journal_fehler_beendet_den_lauf() {
    let a = aufbau();
    datei(&a.q, "a.txt", "a");
    datei(&a.q, "b.txt", "b");
    datei(&a.q, "c.txt", "c");
    let plan = a.plan();
    let mut n = 0;
    let l = laufen_mit(&a, &plan, &opt(1), &AtomicBool::new(false), &mut |_, _| {
        n += 1;
        Err(StellwerkError::Datenbank(format!("Platte voll {n}")))
    });
    assert_eq!(l.erg.journal_fehler.as_deref(), Some("Datenbank: Platte voll 1"));
    assert_eq!((l.erg.erledigt, l.erg.uebersprungen), (1, 2));
    assert_eq!(l.status()[&1], (JournalStatus::Uebersprungen, Some("Lauf vorzeitig beendet".into())));
    assert!(!l.erg.vollstaendig());
}

#[test]
fn fortschritt_wird_gedrosselt_die_letzte_meldung_kommt_immer() {
    let a = aufbau();
    webseite(&a);
    let plan = a.plan();
    let l = laufen(&a, &plan, &Optionen { fortschritt_abstand: Duration::from_secs(3600), ..opt(2) });
    assert_eq!(l.fortschritt.len(), 2);
    assert_eq!(l.fortschritt[1].fertig, plan.operationen.len());
}

#[test]
fn worker_standard_und_ergebnis_als_json() {
    assert!((1..=MAX_WORKER).contains(&standard_worker()));
    let a = aufbau();
    datei(&a.q, "a.txt", "a");
    let plan = a.plan();
    fs::remove_file(a.q.join("a.txt")).unwrap();
    let l = laufen(&a, &plan, &Optionen { worker: 0, ..opt(0) });
    let j = serde_json::to_value(&l.erg).unwrap();
    assert_eq!(j["fehler"][0]["art"], "kopieren");
    assert_eq!(j["fehler"][0]["pfad"], "a.txt");
    assert_eq!(j["erledigt"], 0);
    let f = serde_json::to_value(l.fortschritt[0]).unwrap();
    assert_eq!(f["gesamt"], 1);
}
