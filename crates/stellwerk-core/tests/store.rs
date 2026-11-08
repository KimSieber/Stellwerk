//! Laufzeitdaten in SQLite: Läufe, Journal, Protokoll (ARCHITEKTUR Kap. 4.2).

use std::fs;

use serde_json::json;
use stellwerk_core::diff::{DiffEintrag, DiffStatus};
use stellwerk_core::model::{Dateimodus, Eintrag, EintragArt, Modus, Rechte, RelPath};
use stellwerk_core::plan::{plan_erstellen, OpArt, Plan, PlanZusammenfassung};
use stellwerk_core::model::Hash;
use stellwerk_core::store::{
    basis36, jetzt_ms, Datenbank, JournalStatus, LaufPhase, ManifestEintrag, ProtokollEbene, ProtokollFilter, Protokollieren,
    DATEINAME,
};

const T: i64 = 1_794_000_000_000;

fn p(s: &str) -> RelPath {
    RelPath::new(s).unwrap()
}

fn zeile(pfad: &str, status: DiffStatus, art: EintragArt, groesse: u64) -> DiffEintrag {
    let e = Eintrag { pfad: p(pfad), art, groesse, mtime_ns: None, nur_online: false, hash: None, warnungen: vec![] };
    let (quelle, ziel) = if status == DiffStatus::Loeschen { (None, Some(e)) } else { (Some(e), None) };
    DiffEintrag { pfad: p(pfad), status, quelle, ziel, regel: None, warnungen: vec![], ausgewaehlt: true }
}

/// Ordner anlegen, Datei kopieren, Rechte setzen, Datei löschen.
fn plan() -> Plan {
    let e = vec![
        zeile("web", DiffStatus::Neu, EintragArt::Ordner, 0),
        zeile("web/a.php", DiffStatus::Neu, EintragArt::Datei, 12),
        zeile("alt.txt", DiffStatus::Loeschen, EintragArt::Datei, 3),
    ];
    let rechte = Rechte::Fest { ordner: None, dateien: Dateimodus::new(0o644) };
    plan_erstellen(&e, &rechte).unwrap()
}

fn db() -> Datenbank {
    Datenbank::im_speicher().unwrap()
}

// --- Öffnen und Schema ----------------------------------------------------

#[test]
fn legt_ordner_und_datei_an_und_behaelt_daten() {
    let tmp = tempfile::tempdir().unwrap();
    let ordner = tmp.path().join("App Support/de.sieber-bw.stellwerk");
    let id = {
        let mut d = Datenbank::im_ordner(&ordner).unwrap();
        d.lauf_anlegen("web", "spiegel", Modus::Direkt, &plan(), T).unwrap()
    };
    assert!(ordner.join(DATEINAME).is_file());
    let d = Datenbank::im_ordner(&ordner).unwrap();
    assert_eq!(d.lauf(&id).unwrap().unwrap().strecke_id, "web");
    assert_eq!(d.journal(&id).unwrap().len(), 4);
}

#[test]
fn neueres_schema_wird_nicht_angefasst() {
    let tmp = tempfile::tempdir().unwrap();
    let datei = tmp.path().join("x.sqlite");
    Datenbank::oeffnen(&datei).unwrap().verbindung().pragma_update(None, "user_version", 99).unwrap();
    let e = Datenbank::oeffnen(&datei).err().unwrap();
    assert_eq!(e.kategorie(), "datenbank");
    assert!(e.to_string().contains("Schema 99, diese Version kennt nur bis 4"), "{e}");
    assert!(e.to_string().contains("x.sqlite"), "{e}");
}

#[test]
fn nicht_oeffenbare_datei_und_ordner() {
    let tmp = tempfile::tempdir().unwrap();
    // Ein Ordner ist keine Datenbankdatei
    let e = Datenbank::oeffnen(tmp.path()).err().unwrap();
    assert_eq!(e.kategorie(), "datenbank");
    assert!(e.to_string().contains(&tmp.path().display().to_string()), "{e}");
    // Ordner kann nicht angelegt werden, weil dort eine Datei liegt
    let datei = tmp.path().join("datei");
    fs::write(&datei, "x").unwrap();
    assert_eq!(Datenbank::im_ordner(&datei.join("unter")).err().unwrap().kategorie(), "io");
}

// --- Läufe und Journal -----------------------------------------------------

#[test]
fn lauf_mit_journal_anlegen() {
    let mut d = db();
    let plan = plan();
    let id = d.lauf_anlegen("web", "spiegel", Modus::Direkt, &plan, T).unwrap();
    assert_eq!(id, basis36(T as u64));

    let lauf = d.lauf(&id).unwrap().unwrap();
    assert_eq!((lauf.strecke_id.as_str(), lauf.stufe_id.as_str()), ("web", "spiegel"));
    assert_eq!((lauf.modus, lauf.phase, lauf.gestartet, lauf.beendet), (Modus::Direkt, LaufPhase::Geplant, T, None));
    assert_eq!(lauf.zusammenfassung, plan.zusammenfassung);
    assert_eq!(lauf.ergebnis, None);

    let j = d.journal(&id).unwrap();
    let kurz: Vec<(usize, OpArt, &str, Option<&str>, Option<&str>, u64)> = j
        .iter()
        .map(|z| (z.nr, z.op, z.pfad.as_str(), z.quelle.as_ref().map(RelPath::as_str), z.modus.as_deref(), z.groesse))
        .collect();
    assert_eq!(
        kurz,
        vec![
            (0, OpArt::OrdnerAnlegen, "web", None, None, 0),
            (1, OpArt::Kopieren, "web/a.php", Some("web/a.php"), None, 12),
            (2, OpArt::RechteSetzen, "web/a.php", None, Some("644"), 0),
            (3, OpArt::DateiLoeschen, "alt.txt", None, None, 0),
        ]
    );
    assert!(j.iter().all(|z| z.status == JournalStatus::Offen && z.fehler.is_none() && z.aktualisiert == T));
}

#[test]
fn gleiche_startzeit_ergibt_naechste_freie_kennung() {
    let mut d = db();
    let a = d.lauf_anlegen("web", "spiegel", Modus::Direkt, &plan(), T).unwrap();
    let b = d.lauf_anlegen("web", "spiegel", Modus::Direkt, &plan(), T).unwrap();
    let c = d.lauf_anlegen("web", "server", Modus::Umschalten, &plan(), T).unwrap();
    assert_eq!([a, b, c], [basis36(T as u64), basis36(T as u64 + 1), basis36(T as u64 + 2)]);
}

#[test]
fn leerer_plan_ist_ein_lauf_ohne_journal() {
    let mut d = db();
    let leer = plan_erstellen(&[], &Rechte::NichtSetzen).unwrap();
    let id = d.lauf_anlegen("web", "spiegel", Modus::Direkt, &leer, T).unwrap();
    assert!(d.journal(&id).unwrap().is_empty());
    assert_eq!(d.lauf(&id).unwrap().unwrap().zusammenfassung, PlanZusammenfassung::default());
}

#[test]
fn phase_und_ergebnis() {
    let mut d = db();
    let id = d.lauf_anlegen("web", "spiegel", Modus::Direkt, &plan(), T).unwrap();

    d.lauf_phase(&id, LaufPhase::Laeuft, T + 5, None).unwrap();
    let l = d.lauf(&id).unwrap().unwrap();
    assert_eq!((l.phase, l.beendet), (LaufPhase::Laeuft, None));

    let ergebnis = json!({ "erledigt": 3, "fehler": 1 });
    d.lauf_phase(&id, LaufPhase::Fertig, T + 9, Some(&ergebnis)).unwrap();
    let l = d.lauf(&id).unwrap().unwrap();
    assert_eq!((l.phase, l.beendet, l.ergebnis.clone()), (LaufPhase::Fertig, Some(T + 9), Some(ergebnis.clone())));

    // Ohne neues Ergebnis bleibt das alte stehen
    d.lauf_phase(&id, LaufPhase::Abgebrochen, T + 10, None).unwrap();
    assert_eq!(d.lauf(&id).unwrap().unwrap().ergebnis, Some(ergebnis));

    let e = d.lauf_phase("gibtsnicht", LaufPhase::Fertig, T, None).unwrap_err();
    assert_eq!(e.kategorie(), "nicht_gefunden");
    assert!(e.to_string().contains("Lauf „gibtsnicht“"), "{e}");
    assert!(d.lauf("gibtsnicht").unwrap().is_none());
}

#[test]
fn phasen_beendet() {
    let beendet: Vec<bool> = [LaufPhase::Geplant, LaufPhase::Laeuft, LaufPhase::Fertig, LaufPhase::Abgebrochen]
        .map(LaufPhase::ist_beendet)
        .to_vec();
    assert_eq!(beendet, vec![false, false, true, true]);
}

#[test]
fn journal_aktualisieren() {
    let mut d = db();
    let id = d.lauf_anlegen("web", "spiegel", Modus::Direkt, &plan(), T).unwrap();
    d.journal_setzen(&id, 0, JournalStatus::Erledigt, None, Some("Datenverbindung 1-mal mit neuem Port wiederholt"), T + 1).unwrap();
    d.journal_setzen(&id, 1, JournalStatus::Fehler, Some("Zugriff verweigert"), None, T + 2).unwrap();
    d.journal_setzen(&id, 2, JournalStatus::Uebersprungen, None, None, T + 3).unwrap();
    let j = d.journal(&id).unwrap();
    let stand: Vec<_> = j.iter().map(|z| (z.status, z.fehler.as_deref(), z.hinweis.as_deref(), z.aktualisiert)).collect();
    assert_eq!(
        stand,
        vec![
            (JournalStatus::Erledigt, None, Some("Datenverbindung 1-mal mit neuem Port wiederholt"), T + 1),
            (JournalStatus::Fehler, Some("Zugriff verweigert"), None, T + 2),
            (JournalStatus::Uebersprungen, None, None, T + 3),
            (JournalStatus::Offen, None, None, T),
        ]
    );
    let e = d.journal_setzen(&id, 4, JournalStatus::Erledigt, None, None, T).unwrap_err();
    assert!(e.to_string().contains("Journal-Eintrag 4 von Lauf"), "{e}");
}

#[test]
fn laeufe_einer_stufe_neueste_zuerst() {
    let mut d = db();
    let a = d.lauf_anlegen("web", "spiegel", Modus::Direkt, &plan(), T).unwrap();
    let b = d.lauf_anlegen("web", "spiegel", Modus::Direkt, &plan(), T + 1000).unwrap();
    let c = d.lauf_anlegen("web", "spiegel", Modus::Direkt, &plan(), T + 2000).unwrap();
    d.lauf_anlegen("web", "server", Modus::Direkt, &plan(), T + 3000).unwrap();
    d.lauf_anlegen("andere", "spiegel", Modus::Direkt, &plan(), T + 4000).unwrap();
    let ids = |n| d.laeufe("web", "spiegel", n).unwrap().into_iter().map(|l| l.id).collect::<Vec<_>>();
    assert_eq!(ids(10), vec![c.clone(), b.clone(), a]);
    assert_eq!(ids(2), vec![c, b]);
}

// --- Protokoll -------------------------------------------------------------

#[test]
fn protokoll_schreiben_und_filtern() {
    let d = db();
    let a = d
        .protokollieren(&Protokollieren { zeit: T, strecke_id: Some("web"), aktion: "lauf_gestartet", ..Default::default() })
        .unwrap();
    let b = d
        .protokollieren(&Protokollieren {
            zeit: T + 1,
            ebene: Some(ProtokollEbene::Fehler),
            strecke_id: Some("web"),
            stufe_id: Some("spiegel"),
            lauf_id: Some("l1"),
            aktion: "datei_fehler",
            pfad: Some("web/a.php"),
            details: Some(json!({ "meldung": "Zugriff verweigert" })),
        })
        .unwrap();
    let c = d
        .protokollieren(&Protokollieren {
            zeit: T + 2,
            ebene: Some(ProtokollEbene::Warnung),
            strecke_id: Some("andere"),
            stufe_id: Some("spiegel"),
            aktion: "regel_gesetzt",
            ..Default::default()
        })
        .unwrap();
    assert!(a < b && b < c);

    let ids = |f: ProtokollFilter| d.protokoll(&f).unwrap().into_iter().map(|e| e.id).collect::<Vec<_>>();
    assert_eq!(ids(ProtokollFilter::default()), vec![c, b, a]);
    assert_eq!(ids(ProtokollFilter { strecke_id: Some("web".into()), ..Default::default() }), vec![b, a]);
    assert_eq!(ids(ProtokollFilter { stufe_id: Some("spiegel".into()), ..Default::default() }), vec![c, b]);
    assert_eq!(ids(ProtokollFilter { lauf_id: Some("l1".into()), ..Default::default() }), vec![b]);
    assert_eq!(ids(ProtokollFilter { vor_id: Some(c), limit: 1, ..Default::default() }), vec![b]);

    let alle = d.protokoll(&ProtokollFilter::default()).unwrap();
    assert_eq!(alle[2].ebene, ProtokollEbene::Info, "ohne Angabe: info");
    assert_eq!((alle[2].stufe_id.as_deref(), alle[2].details.as_ref()), (None, None));
    let e = &alle[1];
    assert_eq!((e.zeit, e.ebene, e.aktion.as_str()), (T + 1, ProtokollEbene::Fehler, "datei_fehler"));
    assert_eq!((e.lauf_id.as_deref(), e.pfad.as_deref()), (Some("l1"), Some("web/a.php")));
    assert_eq!(e.details, Some(json!({ "meldung": "Zugriff verweigert" })));
    assert_eq!(alle[0].ebene, ProtokollEbene::Warnung);
}

#[test]
fn protokoll_standardmenge_200() {
    let d = db();
    for i in 0..205 {
        d.protokollieren(&Protokollieren { zeit: T + i, aktion: "x", ..Default::default() }).unwrap();
    }
    assert_eq!(d.protokoll(&ProtokollFilter::default()).unwrap().len(), 200);
    assert_eq!(d.protokoll(&ProtokollFilter { limit: 205, ..Default::default() }).unwrap().len(), 205);
}

#[test]
fn protokollfilter_aus_json() {
    let f: ProtokollFilter = serde_json::from_value(json!({ "strecke_id": "web", "limit": 5 })).unwrap();
    assert_eq!(f, ProtokollFilter { strecke_id: Some("web".into()), limit: 5, ..Default::default() });
}

// --- Beschädigte Daten werden gemeldet, nicht verschluckt --------------------

#[test]
fn beschaedigte_daten_sind_ein_fehler() {
    let mut d = db();
    let id = d.lauf_anlegen("web", "spiegel", Modus::Direkt, &plan(), T).unwrap();
    d.protokollieren(&Protokollieren { zeit: T, aktion: "x", details: Some(json!(1)), ..Default::default() }).unwrap();
    let sql = |s: &str| d.verbindung().execute(s, []).unwrap();

    let fehler = |d: &Datenbank| -> Vec<bool> {
        vec![d.journal(&id).is_err(), d.lauf(&id).is_err(), d.protokoll(&ProtokollFilter::default()).is_err()]
    };
    assert_eq!(fehler(&d), vec![false, false, false]);

    sql("UPDATE journal SET status = 'quatsch' WHERE nr = 0");
    let e = d.journal(&id).unwrap_err();
    assert_eq!(e.kategorie(), "datenbank");
    sql("UPDATE journal SET status = 'offen'");
    sql("UPDATE journal SET pfad = '/absolut' WHERE nr = 0");
    assert!(d.journal(&id).is_err());
    sql("UPDATE journal SET pfad = 'web'");
    sql("UPDATE journal SET quelle = '../raus' WHERE nr = 1");
    assert!(d.journal(&id).is_err());
    sql("UPDATE journal SET quelle = NULL");
    assert!(d.journal(&id).is_ok());

    sql("UPDATE lauf SET zusammenfassung = 'kein json'");
    assert!(d.lauf(&id).is_err());
    sql("UPDATE lauf SET zusammenfassung = '{\"falsch\": 1}'");
    assert!(d.laeufe("web", "spiegel", 5).is_err());

    sql("UPDATE protokoll SET details = '{kaputt'");
    assert!(d.protokoll(&ProtokollFilter::default()).is_err());
}

// --- Hilfen ----------------------------------------------------------------

#[test]
fn basis36_kennungen() {
    assert_eq!(basis36(0), "0");
    assert_eq!(basis36(35), "z");
    assert_eq!(basis36(36), "10");
    assert_eq!(basis36(36 * 36 - 1), "zz");
    assert_eq!(basis36(1_794_000_000_000), "mw5ha39c");
}

#[test]
fn jetzt_ist_nach_2026() {
    assert!(jetzt_ms() > 1_767_225_600_000);
}

#[test]
fn lauf_als_json() {
    let mut d = db();
    let id = d.lauf_anlegen("web", "spiegel", Modus::Direkt, &plan(), T).unwrap();
    let l = serde_json::to_value(d.lauf(&id).unwrap().unwrap()).unwrap();
    assert_eq!((l["phase"].clone(), l["modus"].clone()), (json!("geplant"), json!("direkt")));
    let j = serde_json::to_value(&d.journal(&id).unwrap()[1]).unwrap();
    assert_eq!((j["op"].clone(), j["status"].clone()), (json!("kopieren"), json!("offen")));
}

// --- Schema 2: Manifest und Gelerntes (v0.4) ------------------------------------

#[test]
fn datenbank_aus_v0_3_wird_hochgestuft_laeufe_bleiben() {
    let tmp = tempfile::tempdir().unwrap();
    let datei = tmp.path().join("alt.sqlite");
    let id = {
        let mut d = Datenbank::oeffnen(&datei).unwrap();
        let id = d.lauf_anlegen("web", "spiegel", Modus::Direkt, &plan(), T).unwrap();
        // Zustand wie v0.3: Schema 1 ohne die neuen Tabellen
        d.verbindung().execute_batch("DROP TABLE manifest; DROP TABLE stufen_status; ALTER TABLE journal DROP COLUMN hinweis; PRAGMA user_version = 1;").unwrap();
        id
    };
    let d = Datenbank::oeffnen(&datei).unwrap();
    let v: i64 = d.verbindung().pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
    assert_eq!(v, 4);
    assert_eq!(d.journal(&id).unwrap().len(), 4, "Läufe bleiben erhalten");
    assert!(d.manifest("web", "server").unwrap().is_empty());
    d.status_setzen("web", "server", "k", "v", T).unwrap();
}

fn m(pfad: &str, groesse: u64) -> ManifestEintrag {
    ManifestEintrag {
        pfad: p(pfad),
        ist_ordner: groesse == 0,
        groesse,
        hash: (groesse > 0).then(|| Hash([groesse as u8; 32])),
        mtime_ns: Some(T),
        lauf_id: Some("l1".into()),
    }
}

#[test]
fn datenbank_mit_schema_2_wird_hochgestuft_manifest_bleibt() {
    let tmp = tempfile::tempdir().unwrap();
    let datei = tmp.path().join("v2.sqlite");
    {
        let d = Datenbank::oeffnen(&datei).unwrap();
        // Zustand nach Paket 3: Spalte hieß mtime_ms
        d.verbindung()
            .execute_batch(
                "ALTER TABLE manifest RENAME COLUMN mtime_ns TO mtime_ms; ALTER TABLE journal DROP COLUMN hinweis; PRAGMA user_version = 2;
                 INSERT INTO manifest VALUES ('s', 'server', 'a', 0, 1, NULL, 5, NULL);",
            )
            .unwrap();
    }
    let d = Datenbank::oeffnen(&datei).unwrap();
    assert_eq!(d.manifest("s", "server").unwrap()[0].mtime_ns, Some(5));
}

#[test]
fn datenbank_mit_schema_3_erhaelt_hinweise_im_journal() {
    let tmp = tempfile::tempdir().unwrap();
    let datei = tmp.path().join("v3.sqlite");
    let id = {
        let mut d = Datenbank::oeffnen(&datei).unwrap();
        let id = d.lauf_anlegen("web", "spiegel", Modus::Direkt, &plan(), T).unwrap();
        d.journal_setzen(&id, 1, JournalStatus::Fehler, Some("weg"), None, T + 1).unwrap();
        // Zustand wie v0.4: Journal ohne Hinweis
        d.verbindung().execute_batch("ALTER TABLE journal DROP COLUMN hinweis; PRAGMA user_version = 3;").unwrap();
        id
    };
    let d = Datenbank::oeffnen(&datei).unwrap();
    let j = d.journal(&id).unwrap();
    assert_eq!((j[1].fehler.as_deref(), j[1].hinweis.as_deref()), (Some("weg"), None), "Journal bleibt erhalten");
    d.journal_setzen(&id, 0, JournalStatus::Erledigt, None, Some("h"), T + 2).unwrap();
    assert_eq!(d.journal(&id).unwrap()[0].hinweis.as_deref(), Some("h"));
}

#[test]
fn manifest_setzen_ersetzen_entfernen() {
    let mut d = db();
    for e in [m("web", 0), m("web/a.php", 12), m("web/_x", 3), m("Web/b.txt", 4), m("web2/c", 5), m("alt.txt", 7)] {
        d.manifest_setzen("s", "server", &e).unwrap();
    }
    d.manifest_setzen("s", "andere", &m("web/a.php", 1)).unwrap();
    // Ersetzen eines Pfads
    d.manifest_setzen("s", "server", &ManifestEintrag { hash: None, mtime_ns: None, lauf_id: None, ..m("alt.txt", 8) }).unwrap();
    let pfade = |d: &Datenbank, st: &str| d.manifest("s", st).unwrap().into_iter().map(|e| e.pfad.as_str().to_string()).collect::<Vec<_>>();
    assert_eq!(pfade(&d, "server"), ["Web/b.txt", "alt.txt", "web", "web/_x", "web/a.php", "web2/c"]);
    let alt = d.manifest("s", "server").unwrap().into_iter().find(|e| e.pfad.as_str() == "alt.txt").unwrap();
    assert_eq!(alt, ManifestEintrag { hash: None, mtime_ns: None, lauf_id: None, ..m("alt.txt", 8) });
    assert_eq!(d.manifest("s", "server").unwrap()[4], m("web/a.php", 12), "Hash kommt unverändert zurück");

    // Ordner entfernen: nur er und sein Inhalt – nicht „Web/“ (Groß/Klein) und nicht „web2“
    d.manifest_entfernen("s", "server", &p("web")).unwrap();
    assert_eq!(pfade(&d, "server"), ["Web/b.txt", "alt.txt", "web2/c"]);
    d.manifest_entfernen("s", "server", &p("alt.txt")).unwrap();
    assert_eq!(pfade(&d, "server"), ["Web/b.txt", "web2/c"]);

    d.manifest_ersetzen("s", "server", &[m("neu.txt", 1)]).unwrap();
    assert_eq!(pfade(&d, "server"), ["neu.txt"]);
    assert_eq!(pfade(&d, "andere"), ["web/a.php"], "andere Stufen bleiben unberührt");
}

#[test]
fn manifest_ersetzen_ist_ganz_oder_gar_nicht() {
    let mut d = db();
    d.manifest_setzen("s", "server", &m("bleibt.txt", 1)).unwrap();
    // Tabelle so ändern, dass das Einfügen scheitert
    d.verbindung().execute_batch("CREATE TRIGGER nein BEFORE INSERT ON manifest WHEN NEW.pfad = 'kaputt' BEGIN SELECT RAISE(ABORT, 'nein'); END;").unwrap();
    let e = d.manifest_ersetzen("s", "server", &[m("a", 1), m("kaputt", 1)]).unwrap_err();
    assert_eq!(e.kategorie(), "datenbank");
    let pfade: Vec<_> = d.manifest("s", "server").unwrap().into_iter().map(|e| e.pfad.as_str().to_string()).collect();
    assert_eq!(pfade, ["bleibt.txt"]);
    assert_eq!(d.manifest_setzen("s", "server", &m("kaputt", 1)).unwrap_err().kategorie(), "datenbank");
}

#[test]
fn beschaedigtes_manifest_wird_gemeldet() {
    let d = db();
    d.manifest_setzen("s", "server", &m("a", 1)).unwrap();
    d.verbindung().execute("UPDATE manifest SET hash = x'0102'", []).unwrap();
    assert_eq!(d.manifest("s", "server").unwrap_err().kategorie(), "datenbank");
    d.verbindung().execute("UPDATE manifest SET hash = NULL, pfad = '../raus'", []).unwrap();
    assert_eq!(d.manifest("s", "server").unwrap_err().kategorie(), "datenbank");
    d.verbindung().execute_batch("DROP TABLE manifest").unwrap();
    for e in [d.manifest("s", "x").unwrap_err(), d.manifest_entfernen("s", "x", &p("a")).unwrap_err()] {
        assert_eq!(e.kategorie(), "datenbank");
    }
}

#[test]
fn gelerntes_je_stufe() {
    let d = db();
    assert_eq!(d.status("s", "server", "umbenennen_ueberschreibt").unwrap(), None);
    d.status_setzen("s", "server", "umbenennen_ueberschreibt", "nein", T).unwrap();
    d.status_setzen("s", "server", "umbenennen_ueberschreibt", "ja", T + 1).unwrap();
    d.status_setzen("s", "andere", "umbenennen_ueberschreibt", "nein", T).unwrap();
    assert_eq!(d.status("s", "server", "umbenennen_ueberschreibt").unwrap().as_deref(), Some("ja"));
    d.verbindung().execute_batch("DROP TABLE stufen_status").unwrap();
    assert_eq!(d.status("s", "server", "x").unwrap_err().kategorie(), "datenbank");
    assert_eq!(d.status_setzen("s", "server", "x", "y", T).unwrap_err().kategorie(), "datenbank");
}
