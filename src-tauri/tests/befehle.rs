//! Befehle über die echte IPC-Schicht von Tauri (Mock-Laufzeit, kein Fenster).
//! Geprüft wird die Strecke Frontend-Aufruf → Befehl → Kern → Antwort,
//! einschließlich Parameternamen und JSON-Form, wie sie das Frontend erwartet.

use serde_json::{json, Value};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::{Manager, WebviewWindow, WebviewWindowBuilder};

use stellwerk_core::config::ConfigStore;
use stellwerk_core::secrets::SpeicherTresor;
use stellwerk_lib::{befehle, state::AppState};

/// Beispielstrecke im JSON-Format, wie das Frontend sie sendet; die lokalen
/// Ordner liegen unter `basis` und existieren.
fn strecke_json(basis: &std::path::Path) -> Value {
    let quelle = basis.join("daten/quelle");
    let ziel = basis.join("daten/spiegel");
    std::fs::create_dir_all(&quelle).unwrap();
    std::fs::create_dir_all(&ziel).unwrap();
    json!({
        "id": "mailwerk-web",
        "name": "Mailwerk Webseite",
        "version": 1,
        "regeln": { "standard_aus": [".git/"], "eigene": [{ "muster": "*.psd", "typ": "ignorieren" }] },
        "stufen": [
            {
                "id": "spiegel", "name": "Entwicklung → Spiegel", "modus": "direkt",
                "quelle": { "typ": "lokal", "pfad": quelle },
                "ziel":   { "typ": "lokal", "pfad": ziel },
                "nur_online": "herunterladen"
            },
            {
                "id": "server", "name": "Spiegel → Server", "modus": "umschalten",
                "quelle": { "typ": "stufe", "stufe": "spiegel" },
                "ziel": { "typ": "ftps", "host": "ftp.example.de", "port": 21, "pfad": "/httpdocs",
                          "benutzer": "kim", "tls": "explizit", "konto_ref": "stellwerk:mailwerk-web:server" },
                "rechte": { "modus": "fest", "ordner": "755", "dateien": "644" },
                "nur_online": "herunterladen"
            }
        ]
    })
}

struct Testapp {
    _tmp: tempfile::TempDir,
    wurzel: std::path::PathBuf,
    fenster: WebviewWindow<tauri::test::MockRuntime>,
    _app: tauri::App<tauri::test::MockRuntime>,
}

fn app() -> Testapp {
    let tmp = tempfile::tempdir().unwrap();
    let wurzel = tmp.path().to_path_buf();
    let app = befehle(mock_builder()).build(mock_context(noop_assets())).unwrap();
    app.manage(AppState::new(ConfigStore::new(&wurzel), Box::new(SpeicherTresor::default())).unwrap());
    let fenster = WebviewWindowBuilder::new(&app, "main", Default::default()).build().unwrap();
    Testapp { _tmp: tmp, wurzel, fenster, _app: app }
}

impl Testapp {
    fn aufruf(&self, cmd: &str, args: Value) -> Result<Value, Value> {
        aufruf_im(&self.fenster, cmd, args)
    }
}

/// Befehl über die IPC-Schicht eines Fensters (auch aus einem anderen Thread).
fn aufruf_im(fenster: &WebviewWindow<tauri::test::MockRuntime>, cmd: &str, args: Value) -> Result<Value, Value> {
    let req = InvokeRequest {
        cmd: cmd.into(),
        callback: CallbackFn(0),
        error: CallbackFn(1),
        url: "tauri://localhost".parse().unwrap(),
        body: InvokeBody::Json(args),
        headers: Default::default(),
        invoke_key: INVOKE_KEY.to_string(),
    };
    get_ipc_response(fenster, req).map(|b| b.deserialize::<Value>().unwrap())
}

#[test]
fn konfig_ort_liefert_wurzel() {
    let a = app();
    assert_eq!(a.aufruf("konfig_ort", json!({})).unwrap(), json!(a.wurzel.display().to_string()));
}

#[test]
fn leere_liste() {
    let a = app();
    assert_eq!(a.aufruf("strecken_liste", json!({})).unwrap(), json!([]));
}

#[test]
fn speichern_laden_liste_loeschen() {
    let a = app();

    let warnungen = a.aufruf("strecke_speichern", json!({ "strecke": strecke_json(&a.wurzel) })).unwrap();
    assert_eq!(warnungen, json!([]));
    assert!(a.wurzel.join("strecken/mailwerk-web.toml").exists());

    let geladen = a.aufruf("strecke_laden", json!({ "id": "mailwerk-web" })).unwrap();
    assert_eq!(geladen["stufen"][1]["ziel"]["konto_ref"], "stellwerk:mailwerk-web:server");
    assert_eq!(geladen["stufen"][1]["rechte"]["ordner"], "755");
    assert!(geladen["stufen"][0].get("rechte").is_none(), "„nicht setzen“ wird weggelassen");

    let liste = a.aufruf("strecken_liste", json!({})).unwrap();
    assert_eq!(
        liste,
        json!([{
            "id": "mailwerk-web",
            "name": "Mailwerk Webseite",
            "stufen": [
                { "id": "spiegel", "name": "Entwicklung → Spiegel",
                  "quelle": a.wurzel.join("daten/quelle").display().to_string(),
                  "ziel": a.wurzel.join("daten/spiegel").display().to_string() },
                { "id": "server", "name": "Spiegel → Server",
                  "quelle": "Ziel von „spiegel“",
                  "ziel": "ftps://ftp.example.de/httpdocs" }
            ],
            "probleme": [],
            "fehler": null
        }])
    );

    assert_eq!(a.aufruf("strecke_loeschen", json!({ "id": "mailwerk-web" })).unwrap(), Value::Null);
    assert_eq!(a.aufruf("strecken_liste", json!({})).unwrap(), json!([]));
}

#[test]
fn liste_zeigt_warnungen_alte_ftps_quellen_und_defekte_dateien() {
    let a = app();
    let mut s = strecke_json(&a.wurzel);
    s["stufen"][1]["rechte"] = json!({ "modus": "fest", "dateien": "777" });
    let w = a.aufruf("strecke_speichern", json!({ "strecke": s })).unwrap();
    assert_eq!(w[0]["schwere"], "warnung");

    // Ältere Datei mit FTPS als Quelle: wird angezeigt, mit Fehlermeldung
    std::fs::write(
        a.wurzel.join("strecken/alt.toml"),
        r#"id = "alt"
name = "Alt"
[[stufen]]
id = "s"
name = "S"
quelle = { typ = "ftps", host = "quelle.example.de", pfad = "/www", benutzer = "b", konto_ref = "k" }
ziel = { typ = "lokal", pfad = "/tmp/x" }
"#,
    )
    .unwrap();
    std::fs::write(a.wurzel.join("strecken/kaputt.toml"), "id = ").unwrap();

    let liste = a.aufruf("strecken_liste", json!({})).unwrap();
    assert_eq!(liste[0]["id"], "alt");
    assert_eq!(liste[0]["stufen"][0]["quelle"], "ftps://quelle.example.de/www");
    assert_eq!(liste[0]["probleme"][0]["ort"], "stufen[0].quelle.typ");

    assert_eq!(liste[1]["id"], "kaputt");
    assert_eq!(liste[1]["name"], Value::Null);
    assert!(liste[1]["fehler"].as_str().unwrap().starts_with("Konfigurationsfehler"));

    assert_eq!(liste[2]["probleme"][0]["ort"], "stufen[1].rechte");
}

#[test]
fn pruefen_speichert_nichts() {
    let a = app();
    let mut s = strecke_json(&a.wurzel);
    s["name"] = json!("");
    let p = a.aufruf("strecke_pruefen", json!({ "strecke": s })).unwrap();
    assert_eq!(p, json!([{ "schwere": "fehler", "ort": "name", "meldung": "Name fehlt" }]));
    assert!(!a.wurzel.join("strecken").exists());
}

#[test]
fn fehlerkategorien() {
    let a = app();

    let mut s = strecke_json(&a.wurzel);
    s["stufen"][0]["ziel"]["pfad"] = json!("relativ");
    let e = a.aufruf("strecke_speichern", json!({ "strecke": s })).unwrap_err();
    assert_eq!(e["kategorie"], "validierung");
    assert_eq!(e["probleme"][0]["ort"], "stufen[0].ziel.pfad");

    let e = a.aufruf("strecke_laden", json!({ "id": "fehlt" })).unwrap_err();
    assert_eq!(e, json!({ "kategorie": "nicht_gefunden", "meldung": "Nicht gefunden: Strecke 'fehlt'" }));

    std::fs::create_dir_all(a.wurzel.join("strecken")).unwrap();
    std::fs::write(a.wurzel.join("strecken/kaputt.toml"), "id = ").unwrap();
    let e = a.aufruf("strecke_laden", json!({ "id": "kaputt" })).unwrap_err();
    assert_eq!(e["kategorie"], "konfiguration");
    assert!(e.get("probleme").is_none());

    std::fs::create_dir_all(a.wurzel.join("strecken/ordner.toml")).unwrap();
    let e = a.aufruf("strecke_loeschen", json!({ "id": "ordner" })).unwrap_err();
    assert_eq!(e["kategorie"], "io");
}

#[test]
fn defekte_globale_konfiguration_wird_gemeldet() {
    let a = app();
    std::fs::write(a.wurzel.join("config.toml"), "kaputt =").unwrap();
    assert_eq!(a.aufruf("strecken_liste", json!({})).unwrap_err()["kategorie"], "konfiguration");
    assert_eq!(
        a.aufruf("strecke_pruefen", json!({ "strecke": strecke_json(&a.wurzel) })).unwrap_err()["kategorie"],
        "konfiguration"
    );

    assert_eq!(
        a.aufruf("strecke_anlegen", json!({ "strecke": strecke_json(&a.wurzel) })).unwrap_err()["kategorie"],
        "konfiguration"
    );

    std::fs::remove_file(a.wurzel.join("config.toml")).unwrap();
    std::fs::write(a.wurzel.join("strecken"), "").unwrap();
    assert_eq!(a.aufruf("strecken_liste", json!({})).unwrap_err()["kategorie"], "io");
}

#[test]
fn ungueltiges_json_wird_abgewiesen() {
    // Klartext-FTP lässt sich auch über die Oberfläche nicht einschleusen.
    let a = app();
    let mut s = strecke_json(&a.wurzel);
    s["stufen"][1]["ziel"]["tls"] = json!("keins");
    assert!(a.aufruf("strecke_speichern", json!({ "strecke": s })).is_err());
    assert!(!a.wurzel.join("strecken").exists());
}

#[test]
fn anlegen_ueberschreibt_keine_vorhandene_strecke() {
    let a = app();
    assert_eq!(a.aufruf("strecke_anlegen", json!({ "strecke": strecke_json(&a.wurzel) })).unwrap(), json!([]));
    let mut zweite = strecke_json(&a.wurzel);
    zweite["name"] = json!("Andere");
    let e = a.aufruf("strecke_anlegen", json!({ "strecke": zweite })).unwrap_err();
    assert_eq!(e["kategorie"], "vorhanden");
    assert_eq!(a.aufruf("strecke_laden", json!({ "id": "mailwerk-web" })).unwrap()["name"], "Mailwerk Webseite");
}

#[test]
fn ereignisname_fuer_das_menue() {
    assert_eq!(stellwerk_lib::EREIGNIS_UEBER, "menue_ueber");
}

#[test]
fn passwort_wird_hinterlegt_aber_nie_zurueckgegeben() {
    let a = app();
    let konto = json!({ "konto": "stellwerk:mailwerk-web:server" });
    assert_eq!(a.aufruf("passwort_vorhanden", konto.clone()).unwrap(), json!(false));
    let r = a
        .aufruf("passwort_speichern", json!({ "konto": "stellwerk:mailwerk-web:server", "passwort": "geheim" }))
        .unwrap();
    assert_eq!(r, Value::Null, "keine Rückgabe des Passworts");
    assert_eq!(a.aufruf("passwort_vorhanden", konto.clone()).unwrap(), json!(true));
    a.aufruf("passwort_entfernen", konto.clone()).unwrap();
    assert_eq!(a.aufruf("passwort_vorhanden", konto).unwrap(), json!(false));
}

#[test]
fn loeschen_entfernt_auch_die_passwoerter() {
    let a = app();
    a.aufruf("strecke_anlegen", json!({ "strecke": strecke_json(&a.wurzel) })).unwrap();
    a.aufruf("passwort_speichern", json!({ "konto": "stellwerk:mailwerk-web:server", "passwort": "geheim" }))
        .unwrap();
    a.aufruf("strecke_loeschen", json!({ "id": "mailwerk-web" })).unwrap();
    let vorhanden = a.aufruf("passwort_vorhanden", json!({ "konto": "stellwerk:mailwerk-web:server" })).unwrap();
    assert_eq!(vorhanden, json!(false));
}

#[test]
fn defekte_datei_laesst_sich_loeschen() {
    let a = app();
    std::fs::create_dir_all(a.wurzel.join("strecken")).unwrap();
    std::fs::write(a.wurzel.join("strecken/kaputt.toml"), "id = ").unwrap();
    assert_eq!(a.aufruf("strecke_loeschen", json!({ "id": "kaputt" })).unwrap(), Value::Null);
    assert!(!a.wurzel.join("strecken/kaputt.toml").exists());
}

#[test]
fn ist_ordner_fuer_drag_and_drop() {
    let a = app();
    let ordner = a.wurzel.display().to_string();
    std::fs::write(a.wurzel.join("datei.txt"), "x").unwrap();
    let datei = a.wurzel.join("datei.txt").display().to_string();
    assert_eq!(a.aufruf("ist_ordner", json!({ "pfad": ordner })).unwrap(), json!(true));
    assert_eq!(a.aufruf("ist_ordner", json!({ "pfad": datei })).unwrap(), json!(false));
    assert_eq!(a.aufruf("ist_ordner", json!({ "pfad": "/gibt/es/nicht" })).unwrap(), json!(false));
}

#[test]
fn fehlende_ordner_werden_gemeldet_und_verhindern_das_speichern() {
    let a = app();
    let mut s = strecke_json(&a.wurzel);
    s["stufen"][0]["ziel"]["pfad"] = json!(a.wurzel.join("gibt-es-nicht"));

    // Schon beim Prüfen sichtbar
    let p = a.aufruf("strecke_pruefen", json!({ "strecke": s })).unwrap();
    assert_eq!(p, json!([{ "schwere": "fehler", "ort": "stufen[0].ziel.pfad", "meldung": "Ordner existiert nicht" }]));

    // Anlegen und Speichern lehnen ab, nichts wird geschrieben
    for befehl in ["strecke_anlegen", "strecke_speichern"] {
        let e = a.aufruf(befehl, json!({ "strecke": s })).unwrap_err();
        assert_eq!(e["kategorie"], "validierung", "{befehl}");
        assert_eq!(e["probleme"][0]["ort"], "stufen[0].ziel.pfad");
    }
    assert!(!a.wurzel.join("strecken/mailwerk-web.toml").exists());

    // Startseite zeigt es ebenfalls, z. B. wenn ein Ordner später verschwindet
    a.aufruf("strecke_anlegen", json!({ "strecke": strecke_json(&a.wurzel) })).unwrap();
    std::fs::remove_dir(a.wurzel.join("daten/spiegel")).unwrap();
    let liste = a.aufruf("strecken_liste", json!({})).unwrap();
    assert_eq!(liste[0]["probleme"][0]["meldung"], "Ordner existiert nicht");
}

#[test]
fn globale_regeln_fuer_den_editor() {
    let a = app();
    let r = a.aufruf("globale_regeln", json!({})).unwrap();
    assert_eq!(r[0], json!({ "muster": ".DS_Store", "typ": "ignorieren" }));
    assert!(r.as_array().unwrap().iter().any(|x| x["muster"] == ".git/"));

    std::fs::write(a.wurzel.join("config.toml"), "kaputt =").unwrap();
    assert_eq!(a.aufruf("globale_regeln", json!({})).unwrap_err()["kategorie"], "konfiguration");
}

// --- Vergleich (v0.2) -----------------------------------------------------------

/// Kanal-Argument, wie es `@tauri-apps/api` für einen `Channel` sendet.
const KANAL: &str = "__CHANNEL__:1";

/// Legt die Beispielstrecke an und füllt Quelle und Spiegel.
fn mit_vergleichsdaten(a: &Testapp) {
    a.aufruf("strecke_anlegen", json!({ "strecke": strecke_json(&a.wurzel) })).unwrap();
    let q = a.wurzel.join("daten/quelle");
    let z = a.wurzel.join("daten/spiegel");
    std::fs::create_dir_all(q.join("css")).unwrap();
    std::fs::write(q.join("index.php"), "neu").unwrap();
    std::fs::write(q.join("css/site.css"), "c").unwrap();
    std::fs::write(z.join("alt.txt"), "weg").unwrap();
}

fn starten(a: &Testapp) -> Value {
    a.aufruf("vergleich_starten", json!({ "strecke": "mailwerk-web", "stufe": "spiegel", "fortschritt": KANAL }))
        .unwrap()
}

#[test]
fn vergleich_starten_liefert_kennung_und_zusammenfassung() {
    let a = app();
    mit_vergleichsdaten(&a);
    let v = starten(&a);
    assert_eq!(v["vergleich_id"], "mailwerk-web/spiegel#1");
    assert_eq!(v["quelle"], json!(a.wurzel.join("daten/quelle").display().to_string()));
    assert_eq!(v["ziel"], json!(a.wurzel.join("daten/spiegel").display().to_string()));
    assert_eq!(v["zeilen"], 4);
    assert_eq!(v["zusammenfassung"]["neu"], 3);
    assert_eq!(v["zusammenfassung"]["loeschen"], 1);
    assert_eq!(v["zusammenfassung"]["ausgewaehlt"], 4);
    assert_eq!(v["lesefehler"], json!([]));
    let abbrechen = json!({ "strecke": "mailwerk-web", "stufe": "spiegel" });
    assert_eq!(a.aufruf("vergleich_abbrechen", abbrechen).unwrap(), json!(false), "nach dem Ende abgemeldet");
}

#[test]
fn vergleich_fehler_kommen_mit_kategorie() {
    let a = app();
    mit_vergleichsdaten(&a);
    let e = a
        .aufruf("vergleich_starten", json!({ "strecke": "mailwerk-web", "stufe": "server", "fortschritt": KANAL }))
        .unwrap_err();
    // FTPS-Stufe ohne Manifest (seit v0.4 vergleichbar)
    assert_eq!(e["kategorie"], "kein_manifest");
    let e = a
        .aufruf("vergleich_starten", json!({ "strecke": "fehlt", "stufe": "x", "fortschritt": KANAL }))
        .unwrap_err();
    assert_eq!(e["kategorie"], "nicht_gefunden");
}

#[test]
fn defekte_globale_konfiguration_verhindert_den_vergleich() {
    let a = app();
    mit_vergleichsdaten(&a);
    std::fs::write(a.wurzel.join("config.toml"), "kaputt =").unwrap();
    let e = a
        .aufruf("vergleich_starten", json!({ "strecke": "mailwerk-web", "stufe": "spiegel", "fortschritt": KANAL }))
        .unwrap_err();
    assert_eq!(e["kategorie"], "konfiguration");
}

#[test]
fn diff_seite_mit_filter_und_zugeklappten_ordnern() {
    let a = app();
    mit_vergleichsdaten(&a);
    let id = starten(&a)["vergleich_id"].clone();
    let seite = a
        .aufruf("diff_seite", json!({ "vergleichId": id, "offset": 0, "limit": 10, "filter": "alle", "zugeklappt": [] }))
        .unwrap();
    assert_eq!(seite["sichtbar"], 4);
    let pfade: Vec<&str> = seite["zeilen"].as_array().unwrap().iter().map(|z| z["pfad"].as_str().unwrap()).collect();
    assert_eq!(pfade, ["css", "css/site.css", "alt.txt", "index.php"]);
    assert_eq!(seite["zeilen"][0]["ordner"], true);
    assert_eq!(seite["zeilen"][1]["tiefe"], 1);

    let zu = a
        .aufruf(
            "diff_seite",
            json!({ "vergleichId": id, "offset": 1, "limit": 1, "filter": "aenderungen", "zugeklappt": ["css"] }),
        )
        .unwrap();
    assert_eq!(zu["sichtbar"], 3);
    assert_eq!(zu["zeilen"][0]["pfad"], "alt.txt");
}

#[test]
fn auswahl_setzen_liefert_neue_zusammenfassung() {
    let a = app();
    mit_vergleichsdaten(&a);
    let id = starten(&a)["vergleich_id"].clone();
    let z = a.aufruf("auswahl_setzen", json!({ "vergleichId": id, "pfade": ["css"], "an": false })).unwrap();
    assert_eq!(z["ausgewaehlt"], 2);
    let e = a.aufruf("auswahl_setzen", json!({ "vergleichId": id, "pfade": ["fehlt"], "an": true })).unwrap_err();
    assert_eq!(e["kategorie"], "nicht_gefunden");
    let e = a.aufruf("auswahl_setzen", json!({ "vergleichId": id, "pfade": ["../raus"], "an": true })).unwrap_err();
    assert!(e.to_string().contains("pfade"), "ungültiger Pfad wird beim Einlesen abgewiesen: {e}");
}

#[test]
fn unbekannter_oder_geschlossener_vergleich() {
    let a = app();
    mit_vergleichsdaten(&a);
    let id = starten(&a)["vergleich_id"].clone();
    assert_eq!(a.aufruf("vergleich_schliessen", json!({ "vergleichId": id })).unwrap(), Value::Null);
    let e = a
        .aufruf("diff_seite", json!({ "vergleichId": id, "offset": 0, "limit": 1, "filter": "alle", "zugeklappt": [] }))
        .unwrap_err();
    assert_eq!(e["kategorie"], "nicht_gefunden");
    assert!(e["meldung"].as_str().unwrap().contains("bitte neu vergleichen"));
    // Schließen eines unbekannten Vergleichs ist kein Fehler
    assert!(a.aufruf("vergleich_schliessen", json!({ "vergleichId": "x" })).is_ok());
}

#[test]
fn neuer_vergleich_ersetzt_den_alten_derselben_stufe() {
    let a = app();
    mit_vergleichsdaten(&a);
    let erste = starten(&a)["vergleich_id"].clone();
    let zweite = starten(&a)["vergleich_id"].clone();
    assert_eq!(zweite, "mailwerk-web/spiegel#2");
    let sitzungen = &a.fenster.state::<AppState>().vergleiche.sitzungen;
    let ids: Vec<String> = sitzungen.lock().unwrap().keys().cloned().collect();
    assert_eq!(ids, [zweite.as_str().unwrap()], "{erste} ist freigegeben");
}

#[test]
fn abbrechen_trifft_nur_den_laufenden_vergleich_der_stufe() {
    let a = app();
    let v = &a.fenster.state::<AppState>().vergleiche;
    let abbrechen = |stufe: &str| a.aufruf("vergleich_abbrechen", json!({ "strecke": "s", "stufe": stufe })).unwrap();
    assert_eq!(abbrechen("eins"), json!(false), "nichts läuft");

    let schalter = v.anmelden("s/eins");
    assert_eq!(abbrechen("zwei"), json!(false));
    assert!(!schalter.load(std::sync::atomic::Ordering::Relaxed));
    assert_eq!(abbrechen("eins"), json!(true));
    assert!(schalter.load(std::sync::atomic::Ordering::Relaxed));

    v.abmelden("s/eins", &schalter);
    assert_eq!(abbrechen("eins"), json!(false), "abgemeldet");
}

#[test]
fn neuer_vergleich_bricht_den_alten_ab_und_bleibt_angemeldet() {
    let a = app();
    let v = &a.fenster.state::<AppState>().vergleiche;
    let alt = v.anmelden("s/eins");
    let neu = v.anmelden("s/eins");
    assert!(alt.load(std::sync::atomic::Ordering::Relaxed), "alter Vergleich wird abgebrochen");
    v.abmelden("s/eins", &alt); // der alte endet später – darf den neuen nicht abmelden
    assert_eq!(a.aufruf("vergleich_abbrechen", json!({ "strecke": "s", "stufe": "eins" })).unwrap(), json!(true));
    assert!(neu.load(std::sync::atomic::Ordering::Relaxed));
}

#[test]
fn vergleich_starten_ohne_kanal_wird_abgewiesen() {
    let a = app();
    let e = a.aufruf("vergleich_starten", json!({ "strecke": "s", "stufe": "x" })).unwrap_err();
    assert!(e.to_string().contains("fortschritt"), "{e}");
}

// --- Kontextmenü „Regel setzen“ ----------------------------------------------------

fn regeln_der_stufe(a: &Testapp) -> Value {
    a.aufruf("strecke_laden", json!({ "id": "mailwerk-web" })).unwrap()["stufen"][0]["regeln"]["eigene"].clone()
}

#[test]
fn regel_setzen_speichert_und_wirkt_beim_naechsten_vergleich() {
    let a = app();
    mit_vergleichsdaten(&a);
    let ort = json!({ "ebene": "stufe", "stufe": "spiegel" });
    let warnungen = a
        .aufruf(
            "regel_setzen",
            json!({ "strecke": "mailwerk-web", "ort": ort, "pfad": "alt.txt", "ordner": false, "typ": "schuetzen", "ersatz": null }),
        )
        .unwrap();
    assert!(warnungen.is_array());
    assert_eq!(regeln_der_stufe(&a), json!([{ "muster": "/alt.txt", "typ": "schuetzen" }]));

    let id = starten(&a)["vergleich_id"].clone();
    let seite = a
        .aufruf("diff_seite", json!({ "vergleichId": id, "offset": 0, "limit": 10, "filter": "alle", "zugeklappt": [] }))
        .unwrap();
    let alt = seite["zeilen"].as_array().unwrap().iter().find(|z| z["pfad"] == "alt.txt").unwrap().clone();
    assert_eq!(alt["status"], "geschuetzt");
    assert_eq!(alt["regel_entfernbar"], true);

    a.aufruf("regel_entfernen", json!({ "strecke": "mailwerk-web", "ort": ort, "pfad": "alt.txt", "ordner": false }))
        .unwrap();
    assert_eq!(regeln_der_stufe(&a), Value::Null, "leere Liste wird nicht gespeichert");
}

#[test]
fn regel_fuer_die_ganze_strecke_mit_ersatzdatei() {
    let a = app();
    mit_vergleichsdaten(&a);
    a.aufruf(
        "regel_setzen",
        json!({ "strecke": "mailwerk-web", "ort": { "ebene": "strecke" }, "pfad": "index.php", "ordner": false,
                "typ": "ersetzen", "ersatz": "index.server.php" }),
    )
    .unwrap();
    let s = a.aufruf("strecke_laden", json!({ "id": "mailwerk-web" })).unwrap();
    assert_eq!(
        s["regeln"]["eigene"][1],
        json!({ "muster": "/index.php", "typ": "ersetzen", "quelle": "index.server.php" })
    );
}

#[test]
fn regel_setzen_und_entfernen_fehler() {
    let a = app();
    mit_vergleichsdaten(&a);
    let ort = json!({ "ebene": "stufe", "stufe": "spiegel" });
    // unpassende Angabe: Ersetzen ohne Ersatzdatei
    let e = a
        .aufruf(
            "regel_setzen",
            json!({ "strecke": "mailwerk-web", "ort": ort, "pfad": "a", "ordner": false, "typ": "ersetzen", "ersatz": null }),
        )
        .unwrap_err();
    assert_eq!(e["kategorie"], "validierung");
    // nichts zu entfernen
    let e = a
        .aufruf("regel_entfernen", json!({ "strecke": "mailwerk-web", "ort": ort, "pfad": "a", "ordner": false }))
        .unwrap_err();
    assert_eq!(e["kategorie"], "nicht_gefunden");
    // unbekannte Strecke
    let e = a
        .aufruf("regel_entfernen", json!({ "strecke": "fehlt", "ort": ort, "pfad": "a", "ordner": false }))
        .unwrap_err();
    assert_eq!(e["kategorie"], "nicht_gefunden");
    let e = a
        .aufruf(
            "regel_setzen",
            json!({ "strecke": "fehlt", "ort": ort, "pfad": "a", "ordner": false, "typ": "schuetzen", "ersatz": null }),
        )
        .unwrap_err();
    assert_eq!(e["kategorie"], "nicht_gefunden");
}

#[test]
fn regel_wird_nicht_gespeichert_wenn_die_strecke_fehler_hat() {
    let a = app();
    mit_vergleichsdaten(&a);
    let ort = json!({ "ebene": "stufe", "stufe": "spiegel" });
    a.aufruf("regel_setzen", json!({ "strecke": "mailwerk-web", "ort": ort, "pfad": "x", "ordner": false, "typ": "schuetzen", "ersatz": null }))
        .unwrap();
    std::fs::remove_dir_all(a.wurzel.join("daten/spiegel")).unwrap(); // Zielordner fehlt → Fehler der Strecke
    for (befehl, args) in [
        ("regel_setzen", json!({ "strecke": "mailwerk-web", "ort": ort, "pfad": "y", "ordner": false, "typ": "schuetzen", "ersatz": null })),
        ("regel_entfernen", json!({ "strecke": "mailwerk-web", "ort": ort, "pfad": "x", "ordner": false })),
    ] {
        let e = a.aufruf(befehl, args).unwrap_err();
        assert_eq!(e["kategorie"], "validierung", "{befehl}");
    }
    assert_eq!(regeln_der_stufe(&a), json!([{ "muster": "/x", "typ": "schuetzen" }]), "unverändert");
}

#[test]
fn ersatz_vorschlaege_aus_dem_selben_ordner() {
    let a = app();
    mit_vergleichsdaten(&a);
    let id = starten(&a)["vergleich_id"].clone();
    let v = a.aufruf("ersatz_vorschlaege", json!({ "vergleichId": id, "pfad": "css/neu.css" })).unwrap();
    assert_eq!(v, json!(["css/site.css"]));
    let e = a.aufruf("ersatz_vorschlaege", json!({ "vergleichId": "x", "pfad": "a" })).unwrap_err();
    assert_eq!(e["kategorie"], "nicht_gefunden");
}

// --- Übertragen „Direkt“ (v0.3) --------------------------------------------------

/// Vergleicht die Beispielstufe und erstellt den Plan; liefert (Vergleich, Vorschau).
fn vergleichen_und_planen(a: &Testapp) -> (Value, Value) {
    let id = starten(a)["vergleich_id"].clone();
    let vorschau = a.aufruf("plan_erstellen", json!({ "vergleichId": id })).unwrap();
    (id, vorschau)
}

fn uebertragen(a: &Testapp, id: &Value, bestaetigt: bool, stoppen: bool) -> Result<Value, Value> {
    a.aufruf(
        "lauf_starten",
        json!({ "vergleichId": id, "loeschenBestaetigt": bestaetigt, "beiFehlerStoppen": stoppen, "fortschritt": KANAL }),
    )
}

fn db(a: &Testapp) -> std::sync::MutexGuard<'_, stellwerk_core::store::Datenbank> {
    a.fenster.state::<AppState>().inner().datenbank.lock().unwrap()
}

/// (Aktion, Ebene) der Protokolleinträge eines Laufs, älteste zuerst.
fn protokoll(a: &Testapp, lauf: &str) -> Vec<(String, String)> {
    let f = stellwerk_core::store::ProtokollFilter { lauf_id: Some(lauf.into()), ..Default::default() };
    let mut v: Vec<(String, String)> = db(a)
        .protokoll(&f)
        .unwrap()
        .into_iter()
        .map(|e| (e.aktion, serde_json::to_value(e.ebene).unwrap().as_str().unwrap().to_string()))
        .collect();
    v.reverse();
    v
}

#[test]
fn plan_zeigen_uebertragen_danach_alles_gleich() {
    let a = app();
    mit_vergleichsdaten(&a);
    let (id, v) = vergleichen_und_planen(&a);
    assert_eq!(
        v["zusammenfassung"],
        json!({ "ordner_anlegen": 1, "kopieren": 2, "ueberschreiben": 0, "rechte": 0,
                "dateien_loeschen": 1, "ordner_loeschen": 0, "bytes": 4 })
    );
    assert_eq!(v["loeschliste"], json!(["alt.txt"]));
    assert_eq!(v["warnungen"], json!([]));
    assert_eq!(v["ziel"], json!(a.wurzel.join("daten/spiegel").display().to_string()));
    assert!((1..=8).contains(&v["worker"].as_u64().unwrap()));

    let r = uebertragen(&a, &id, true, false).unwrap();
    assert_eq!(r["ergebnis"]["erledigt"], 4);
    assert_eq!(r["ergebnis"]["fehler"], json!([]));
    let z = a.wurzel.join("daten/spiegel");
    assert_eq!(std::fs::read_to_string(z.join("css/site.css")).unwrap(), "c");
    assert!(!z.join("alt.txt").exists());
    let neu = starten(&a);
    let s = neu["zusammenfassung"].clone();
    assert_eq!((s["neu"].clone(), s["geaendert"].clone(), s["loeschen"].clone()), (json!(0), json!(0), json!(0)));

    let lauf = r["lauf_id"].as_str().unwrap();
    let l = db(&a).lauf(lauf).unwrap().unwrap();
    assert_eq!((l.strecke_id.as_str(), l.stufe_id.as_str()), ("mailwerk-web", "spiegel"));
    assert_eq!(l.phase, stellwerk_core::store::LaufPhase::Fertig);
    assert_eq!(l.ergebnis.unwrap()["erledigt"], 4);
    let journal = db(&a).journal(lauf).unwrap();
    assert!(journal.iter().all(|j| j.status == stellwerk_core::store::JournalStatus::Erledigt));
    assert_eq!(journal.len(), 4);
    assert_eq!(protokoll(&a, lauf), vec![("lauf_gestartet".into(), "info".into()), ("lauf_beendet".into(), "info".into())]);

    // Der alte Vergleich ist durch den neuen ersetzt; ein Plan gilt genau einmal
    assert_eq!(uebertragen(&a, &id, true, false).unwrap_err()["kategorie"], "nicht_gefunden");
    let id = neu["vergleich_id"].clone();
    a.aufruf("plan_erstellen", json!({ "vergleichId": id })).unwrap();
    assert_eq!(uebertragen(&a, &id, true, false).unwrap()["ergebnis"]["erledigt"], 0);
    assert_eq!(uebertragen(&a, &id, true, false).unwrap_err()["kategorie"], "nicht_bestaetigt");
}

#[test]
fn loeschungen_muessen_bestaetigt_sein() {
    let a = app();
    mit_vergleichsdaten(&a);
    let (id, _) = vergleichen_und_planen(&a);
    let e = uebertragen(&a, &id, false, false).unwrap_err();
    assert_eq!(e["kategorie"], "nicht_bestaetigt");
    assert!(e["meldung"].as_str().unwrap().contains("1 Löschungen"), "{e}");
    assert!(a.wurzel.join("daten/spiegel/alt.txt").exists(), "nichts verändert");
    assert!(db(&a).laeufe("mailwerk-web", "spiegel", 5).unwrap().is_empty());
}

#[test]
fn ohne_loeschungen_braucht_es_keine_bestaetigung() {
    let a = app();
    mit_vergleichsdaten(&a);
    std::fs::remove_file(a.wurzel.join("daten/spiegel/alt.txt")).unwrap();
    let (id, v) = vergleichen_und_planen(&a);
    assert_eq!(v["loeschliste"], json!([]));
    assert_eq!(uebertragen(&a, &id, false, false).unwrap()["ergebnis"]["erledigt"], 3);
}

#[test]
fn ohne_plan_oder_nach_neuer_auswahl_wird_nicht_uebertragen() {
    let a = app();
    mit_vergleichsdaten(&a);
    let id = starten(&a)["vergleich_id"].clone();
    let e = uebertragen(&a, &id, true, false).unwrap_err();
    assert_eq!(e["kategorie"], "nicht_bestaetigt");
    assert!(e["meldung"].as_str().unwrap().contains("erneut öffnen und bestätigen"));

    a.aufruf("plan_erstellen", json!({ "vergleichId": id })).unwrap();
    a.aufruf("auswahl_setzen", json!({ "vergleichId": id, "pfade": ["alt.txt"], "an": false })).unwrap();
    assert_eq!(uebertragen(&a, &id, true, false).unwrap_err()["kategorie"], "nicht_bestaetigt");

    let e = a.aufruf("plan_erstellen", json!({ "vergleichId": "x" })).unwrap_err();
    assert_eq!(e["kategorie"], "nicht_gefunden");
    let e = uebertragen(&a, &json!("x"), true, false).unwrap_err();
    assert_eq!(e["kategorie"], "nicht_gefunden");
}

#[test]
fn plan_nur_wenn_die_stufe_noch_passt() {
    let a = app();
    mit_vergleichsdaten(&a);
    let id = starten(&a)["vergleich_id"].clone();
    let plan = || a.aufruf("plan_erstellen", json!({ "vergleichId": id })).unwrap_err();
    let mut s = strecke_json(&a.wurzel);

    // Zielordner seit dem Vergleich geändert
    let anderes = a.wurzel.join("daten/anderes");
    std::fs::create_dir_all(&anderes).unwrap();
    s["stufen"][0]["ziel"]["pfad"] = json!(anderes);
    a.aufruf("strecke_speichern", json!({ "strecke": s })).unwrap();
    assert!(plan()["meldung"].as_str().unwrap().contains("seit dem Vergleich geändert"));

    // Stufe umbenannt, dann Strecke gelöscht
    s["stufen"][0]["id"] = json!("anders");
    s["stufen"][1]["quelle"]["stufe"] = json!("anders");
    a.aufruf("strecke_speichern", json!({ "strecke": s })).unwrap();
    assert!(plan()["meldung"].as_str().unwrap().contains("Stufe „spiegel“"));
    a.aufruf("strecke_loeschen", json!({ "id": "mailwerk-web" })).unwrap();
    assert_eq!(plan()["kategorie"], "nicht_gefunden");
}

#[test]
fn fehler_je_datei_stehen_in_journal_und_protokoll() {
    let a = app();
    mit_vergleichsdaten(&a);
    let (id, _) = vergleichen_und_planen(&a);
    std::fs::remove_file(a.wurzel.join("daten/quelle/index.php")).unwrap();
    let r = uebertragen(&a, &id, true, true).unwrap();
    assert_eq!(r["ergebnis"]["fehler"][0]["pfad"], "index.php");
    assert_eq!(r["ergebnis"]["gestoppt"], true);
    let lauf = r["lauf_id"].as_str().unwrap();
    let p = protokoll(&a, lauf);
    assert!(p.contains(&("operation_fehler".into(), "fehler".into())), "{p:?}");
    assert_eq!(p.last().unwrap(), &("lauf_beendet".into(), "fehler".into()));
    let fehler = db(&a).protokoll(&Default::default()).unwrap().into_iter().find(|e| e.aktion == "operation_fehler").unwrap();
    assert_eq!(fehler.pfad.as_deref(), Some("index.php"));
}

#[test]
fn zweite_uebertragung_derselben_stufe_wird_abgewiesen() {
    let a = app();
    mit_vergleichsdaten(&a);
    let (id, _) = vergleichen_und_planen(&a);
    let laeufe = &a.fenster.state::<AppState>().inner().laeufe;
    let schalter = laeufe.anmelden("mailwerk-web/spiegel").unwrap();
    assert!(laeufe.anmelden("mailwerk-web/spiegel").is_none());
    let e = uebertragen(&a, &id, true, false).unwrap_err();
    assert_eq!(e["kategorie"], "vorhanden");
    assert!(a.wurzel.join("daten/spiegel/alt.txt").exists());

    let abbrechen = |stufe: &str| a.aufruf("lauf_abbrechen", json!({ "strecke": "mailwerk-web", "stufe": stufe })).unwrap();
    assert_eq!(abbrechen("server"), json!(false));
    assert_eq!(abbrechen("spiegel"), json!(true));
    assert!(schalter.load(std::sync::atomic::Ordering::Relaxed));
    laeufe.abmelden("mailwerk-web/spiegel");
    assert_eq!(abbrechen("spiegel"), json!(false));
}

#[test]
fn abgebrochene_uebertragung() {
    let a = app();
    mit_vergleichsdaten(&a);
    // Große Datei, damit der Abbruch während des Laufs ankommt
    let block = vec![1u8; 1 << 20];
    let mut f = std::fs::File::create(a.wurzel.join("daten/quelle/gross.bin")).unwrap();
    for _ in 0..64 {
        std::io::Write::write_all(&mut f, &block).unwrap();
    }
    drop(f);
    let (id, _) = vergleichen_und_planen(&a);
    let r = std::thread::scope(|s| {
        let (fenster, id) = (&a.fenster, &id);
        let lauf = s.spawn(move || {
            let args = json!({ "vergleichId": id, "loeschenBestaetigt": true, "beiFehlerStoppen": false, "fortschritt": KANAL });
            aufruf_im(fenster, "lauf_starten", args).unwrap()
        });
        while a.aufruf("lauf_abbrechen", json!({ "strecke": "mailwerk-web", "stufe": "spiegel" })).unwrap() != json!(true) {
            std::thread::yield_now();
        }
        lauf.join().unwrap()
    });
    assert_eq!(r["ergebnis"]["abgebrochen"], true);
    let lauf = r["lauf_id"].as_str().unwrap();
    assert_eq!(db(&a).lauf(lauf).unwrap().unwrap().phase, stellwerk_core::store::LaufPhase::Abgebrochen);
    assert_eq!(protokoll(&a, lauf).last().unwrap(), &("lauf_beendet".into(), "warnung".into()));
}

#[test]
fn lauf_starten_ohne_kanal_wird_abgewiesen() {
    let a = app();
    let e = a.aufruf("lauf_starten", json!({ "vergleichId": "x", "loeschenBestaetigt": true, "beiFehlerStoppen": false }));
    assert!(e.is_err());
}

#[test]
fn ohne_beschreibbaren_konfigurationsordner_startet_nichts() {
    let tmp = tempfile::tempdir().unwrap();
    let datei = tmp.path().join("datei");
    std::fs::write(&datei, "x").unwrap();
    let e = AppState::new(ConfigStore::new(&datei), Box::new(SpeicherTresor::default())).err().unwrap();
    assert_eq!(e.kategorie(), "io");
}

#[test]
fn laeufe_und_journal_fuer_die_streckenansicht() {
    let a = app();
    mit_vergleichsdaten(&a);
    let leer = a.aufruf("laeufe", json!({ "strecke": "mailwerk-web", "stufe": "spiegel", "limit": 10 })).unwrap();
    assert_eq!(leer, json!([]));
    let (id, _) = vergleichen_und_planen(&a);
    let r = uebertragen(&a, &id, true, false).unwrap();

    let l = a.aufruf("laeufe", json!({ "strecke": "mailwerk-web", "stufe": "spiegel", "limit": 10 })).unwrap();
    assert_eq!(l[0]["id"], r["lauf_id"]);
    assert_eq!(l[0]["phase"], "fertig");
    assert_eq!(l[0]["ergebnis"]["erledigt"], 4);
    assert_eq!(l[0]["zusammenfassung"]["kopieren"], 2);
    assert!(l[0]["beendet"].is_i64());

    let j = a.aufruf("lauf_journal", json!({ "laufId": r["lauf_id"] })).unwrap();
    assert_eq!(j.as_array().unwrap().len(), 4);
    assert_eq!(j[0], json!({ "nr": 0, "op": "ordner_anlegen", "pfad": "css", "quelle": null, "modus": null,
                             "groesse": 0, "status": "erledigt", "fehler": null, "hinweis": null,
                             "aktualisiert": j[0]["aktualisiert"] }));
    assert_eq!(a.aufruf("lauf_journal", json!({ "laufId": "gibtsnicht" })).unwrap(), json!([]));
}

#[test]
fn beschaedigte_laufdaten_werden_gemeldet() {
    let a = app();
    mit_vergleichsdaten(&a);
    let (id, _) = vergleichen_und_planen(&a);
    let lauf = uebertragen(&a, &id, true, false).unwrap()["lauf_id"].clone();
    db(&a).verbindung().execute("UPDATE journal SET status = 'quatsch'", []).unwrap();
    db(&a).verbindung().execute("UPDATE lauf SET phase = 'quatsch'", []).unwrap();
    let e = a.aufruf("lauf_journal", json!({ "laufId": lauf })).unwrap_err();
    assert_eq!(e["kategorie"], "datenbank");
    let e = a.aufruf("laeufe", json!({ "strecke": "mailwerk-web", "stufe": "spiegel", "limit": 10 })).unwrap_err();
    assert_eq!(e["kategorie"], "datenbank");
}

// --- Verbindung testen (FTPS, v0.4) ------------------------------------------

fn ftps_ziel(port: u16, zertifikat: Option<&str>) -> Value {
    json!({ "typ": "ftps", "host": "127.0.0.1", "port": port, "pfad": "/", "benutzer": stellwerk_testserver::BENUTZER,
            "tls": "explizit", "konto_ref": "stellwerk:test:server", "zertifikat": zertifikat })
}

#[test]
fn verbindung_testen_meldet_zertifikat_mit_fingerprint_und_gelingt_nach_bestaetigung() {
    let a = app();
    let s = stellwerk_testserver::starten(Default::default());
    let pw = stellwerk_testserver::PASSWORT;

    // Selbstsigniert: Fehler mit Fingerprint zum Bestätigen
    let e = a.aufruf("verbindung_testen", json!({ "ziel": ftps_ziel(s.port, None), "passwort": pw })).unwrap_err();
    assert_eq!(e["kategorie"], "zertifikat");
    assert_eq!(e["fingerprint"], json!(s.fingerprint));

    // Bestätigt, Passwort eingegeben (noch nicht gespeichert)
    let ok = a
        .aufruf("verbindung_testen", json!({ "ziel": ftps_ziel(s.port, Some(&s.fingerprint)), "passwort": pw }))
        .unwrap();
    assert_eq!(ok["verzeichnis"], "/");
    assert_eq!(ok["faehigkeiten"]["mlsd"], true);
    assert_eq!(ok["faehigkeiten"]["chmod"], Value::Null);
    assert_eq!(ok["zertifikat"]["vom_system"], false);
    assert!(!ok.to_string().contains(pw), "Passwort nie in der Antwort");

    // Ohne Eingabe: aus dem Schlüsselbund; leere Eingabe zählt als keine
    let ziel = ftps_ziel(s.port, Some(&s.fingerprint));
    let e = a.aufruf("verbindung_testen", json!({ "ziel": ziel, "passwort": "" })).unwrap_err();
    assert_eq!(e["kategorie"], "nicht_gefunden");
    a.aufruf("passwort_speichern", json!({ "konto": "stellwerk:test:server", "passwort": pw })).unwrap();
    assert!(a.aufruf("verbindung_testen", json!({ "ziel": ziel })).is_ok());

    // Falsches Passwort im Schlüsselbund
    a.aufruf("passwort_speichern", json!({ "konto": "stellwerk:test:server", "passwort": "falsch" })).unwrap();
    let e = a.aufruf("verbindung_testen", json!({ "ziel": ziel })).unwrap_err();
    assert_eq!(e["kategorie"], "anmeldung");
    assert!(e.get("fingerprint").is_none());

    // Lokales Ziel: kein Verbindungstest
    let e = a.aufruf("verbindung_testen", json!({ "ziel": { "typ": "lokal", "pfad": "/tmp" } })).unwrap_err();
    assert_eq!(e["kategorie"], "nicht_unterstuetzt");

    // Unvollständige Angaben werden schon beim Einlesen abgewiesen
    let e = a.aufruf("verbindung_testen", json!({ "ziel": { "typ": "ftps", "host": "h" } })).unwrap_err();
    assert!(e.to_string().contains("ziel"), "{e}");
    let e = a.aufruf("verbindung_testen", json!({ "ziel": ziel, "passwort": 42 })).unwrap_err();
    assert!(e.to_string().contains("passwort"), "{e}");
}

// --- FTPS vergleichen (v0.4 Schritt 2) -----------------------------------------

/// Strecke mit einer Stufe lokal → Testserver; Quelle mit `index.php` (neu) und
/// `gleich.css` (am Server gleich groß, anderer Inhalt).
fn ftps_strecke(a: &Testapp, s: &stellwerk_testserver::TestServer) {
    let q = a.wurzel.join("daten/quelle");
    std::fs::create_dir_all(&q).unwrap();
    std::fs::write(q.join("index.php"), "neu").unwrap();
    std::fs::write(q.join("gleich.css"), "ccc").unwrap();
    std::fs::write(s.wurzel().join("gleich.css"), "xyz").unwrap();
    std::fs::write(s.wurzel().join("alt.txt"), "weg").unwrap();
    let mut ziel = ftps_ziel(s.port, Some(&s.fingerprint));
    ziel["konto_ref"] = json!("stellwerk:ftps-web:server");
    let strecke = json!({
        "id": "ftps-web", "name": "FTPS", "version": 1,
        "stufen": [{ "id": "server", "name": "Server", "modus": "direkt",
                     "quelle": { "typ": "lokal", "pfad": q }, "ziel": ziel }]
    });
    a.aufruf("strecke_anlegen", json!({ "strecke": strecke })).unwrap();
}

fn ftps_vergleich(a: &Testapp, server: Option<bool>) -> Result<Value, Value> {
    a.aufruf("vergleich_starten", json!({ "strecke": "ftps-web", "stufe": "server", "fortschritt": KANAL, "server": server }))
}

#[test]
fn ftps_erst_mit_dem_server_dann_gegen_das_manifest() {
    let a = app();
    let s = stellwerk_testserver::starten(Default::default());
    ftps_strecke(&a, &s);

    // Noch nie eingelesen: kein Serverkontakt, eigene Kategorie
    let e = ftps_vergleich(&a, None).unwrap_err();
    assert_eq!(e["kategorie"], "kein_manifest");
    assert_eq!(ftps_vergleich(&a, Some(false)).unwrap_err()["kategorie"], "kein_manifest");
    // Mit Server, aber ohne Passwort im Schlüsselbund
    assert_eq!(ftps_vergleich(&a, Some(true)).unwrap_err()["kategorie"], "nicht_gefunden");
    assert!(db(&a).manifest("ftps-web", "server").unwrap().is_empty());

    a.aufruf("passwort_speichern", json!({ "konto": "stellwerk:ftps-web:server", "passwort": stellwerk_testserver::PASSWORT }))
        .unwrap();
    let v = ftps_vergleich(&a, Some(true)).unwrap();
    assert_eq!(v["ziel_stand"], "server");
    assert_eq!(v["ziel"], json!(format!("ftps://127.0.0.1:{}/", s.port)));
    assert_eq!(v["zusammenfassung"]["neu"], 1);
    assert_eq!(v["zusammenfassung"]["gleich"], 1);
    assert_eq!(v["zusammenfassung"]["loeschen"], 1);
    let seite = a
        .aufruf("diff_seite", json!({ "vergleichId": v["vergleich_id"], "offset": 0, "limit": 10, "filter": "alle", "zugeklappt": [] }))
        .unwrap();
    let gleich = seite["zeilen"].as_array().unwrap().iter().find(|z| z["pfad"] == "gleich.css").unwrap().clone();
    assert_eq!(gleich["warnungen"], json!([{ "art": "inhalt_nicht_geprueft" }]));

    // Manifest gespeichert (ohne Hashes), Zeitpunkt gemerkt; am Server nichts verändert
    let m = db(&a).manifest("ftps-web", "server").unwrap();
    let pfade: Vec<_> = m.iter().map(|e| (e.pfad.as_str(), e.hash.is_none())).collect();
    assert_eq!(pfade, [("alt.txt", true), ("gleich.css", true)]);
    assert!(db(&a).status("ftps-web", "server", "manifest_seit").unwrap().is_some());
    assert_eq!(std::fs::read_to_string(s.wurzel().join("gleich.css")).unwrap(), "xyz");

    // Danach gegen das Manifest: Änderungen am Server bleiben unsichtbar (kein Kontakt)
    std::fs::remove_file(s.wurzel().join("alt.txt")).unwrap();
    let m = ftps_vergleich(&a, None).unwrap();
    assert_eq!(m["ziel_stand"], "manifest");
    assert_eq!(m["zusammenfassung"]["loeschen"], 1);
    // Server neu lesen gleicht das Manifest ab
    let neu = ftps_vergleich(&a, Some(true)).unwrap();
    assert_eq!(neu["zusammenfassung"]["loeschen"], 0);
    assert_eq!(db(&a).manifest("ftps-web", "server").unwrap().len(), 1);
}

#[test]
fn ftps_fehler_beim_einlesen_speichern_kein_manifest() {
    let a = app();
    let s = stellwerk_testserver::starten(Default::default());
    ftps_strecke(&a, &s);
    a.aufruf("passwort_speichern", json!({ "konto": "stellwerk:ftps-web:server", "passwort": "falsch" })).unwrap();
    assert_eq!(ftps_vergleich(&a, Some(true)).unwrap_err()["kategorie"], "anmeldung");
    assert!(db(&a).status("ftps-web", "server", "manifest_seit").unwrap().is_none());
    // Lokale Stufen ignorieren `server`
    mit_vergleichsdaten(&a);
    let v = a
        .aufruf("vergleich_starten", json!({ "strecke": "mailwerk-web", "stufe": "spiegel", "fortschritt": KANAL, "server": true }))
        .unwrap();
    assert_eq!(v["ziel_stand"], Value::Null);
}

#[test]
fn vergleich_starten_mit_falschem_server_wert_wird_abgewiesen() {
    let a = app();
    let e = a.aufruf("vergleich_starten", json!({ "strecke": "x", "stufe": "y", "fortschritt": KANAL, "server": "ja" }));
    assert!(e.unwrap_err().to_string().contains("server"));
}

// --- FTPS übertragen (v0.4 Schritt 3) -------------------------------------------

fn passwort_hinterlegen(a: &Testapp) {
    a.aufruf("passwort_speichern", json!({ "konto": "stellwerk:ftps-web:server", "passwort": stellwerk_testserver::PASSWORT }))
        .unwrap();
}

/// Mit dem Server vergleichen und den Plan erstellen; liefert (Vergleich, Vorschau).
fn ftps_planen(a: &Testapp, server: bool) -> (Value, Value) {
    let id = ftps_vergleich(a, Some(server)).unwrap()["vergleich_id"].clone();
    let vorschau = a.aufruf("plan_erstellen", json!({ "vergleichId": id })).unwrap();
    (id, vorschau)
}

fn seite(a: &Testapp, id: &Value) -> Vec<Value> {
    let s = a.aufruf("diff_seite", json!({ "vergleichId": id, "offset": 0, "limit": 50, "filter": "alle", "zugeklappt": [] })).unwrap();
    s["zeilen"].as_array().unwrap().clone()
}

fn gelernt(a: &Testapp) -> Option<String> {
    db(a).status("ftps-web", "server", "umbenennen_ueberschreibt").unwrap()
}

#[test]
fn ftps_uebertragen_schreibt_den_server_und_den_letzten_bekannten_stand() {
    let a = app();
    let s = stellwerk_testserver::starten(Default::default());
    ftps_strecke(&a, &s);
    let q = a.wurzel.join("daten/quelle");
    std::fs::create_dir_all(q.join("css")).unwrap();
    std::fs::write(q.join("css/site.css"), "body{}").unwrap();
    passwort_hinterlegen(&a);

    let (id, vorschau) = ftps_planen(&a, true);
    assert_eq!(vorschau["worker"], 2, "FTPS: fest 2 Verbindungen");
    assert_eq!(vorschau["ziel"], json!(format!("ftps://127.0.0.1:{}/", s.port)));
    assert_eq!(vorschau["zusammenfassung"]["kopieren"], 2);
    assert_eq!(vorschau["zusammenfassung"]["dateien_loeschen"], 1);
    let r = uebertragen(&a, &id, true, false).unwrap();
    assert_eq!(r["ergebnis"]["erledigt"], 4, "{r}");
    assert_eq!(r["ergebnis"]["fehler"], json!([]));

    // Am Server: neu, Ordner, gelöscht; „gleich“ (nur Größe) bleibt unberührt
    assert_eq!(std::fs::read_to_string(s.wurzel().join("index.php")).unwrap(), "neu");
    assert_eq!(std::fs::read_to_string(s.wurzel().join("css/site.css")).unwrap(), "body{}");
    assert!(!s.wurzel().join("alt.txt").exists());
    assert_eq!(std::fs::read_to_string(s.wurzel().join("gleich.css")).unwrap(), "xyz");

    // Letzter bekannter Stand: übertragene Dateien mit Hash und Lauf, Gelöschtes heraus
    let lauf = r["lauf_id"].as_str().unwrap();
    let m = db(&a).manifest("ftps-web", "server").unwrap();
    let stand: Vec<_> = m.iter().map(|e| (e.pfad.as_str(), e.ist_ordner, e.hash.is_some(), e.lauf_id.as_deref())).collect();
    assert_eq!(
        stand,
        [("css", true, false, Some(lauf)), ("css/site.css", false, true, Some(lauf)), ("gleich.css", false, false, None), ("index.php", false, true, Some(lauf))]
    );
    assert!(m.iter().find(|e| e.pfad.as_str() == "index.php").unwrap().mtime_ns.is_some());
    // Nichts ersetzt → über das Umbenennen noch nichts gelernt
    assert_eq!(gelernt(&a), None);

    // Schnell vergleichen: alles gleich, übertragene Dateien ohne Hinweis
    let v = ftps_vergleich(&a, Some(false)).unwrap();
    assert_eq!(v["zusammenfassung"]["ausgewaehlt"], 0);
    let zeilen = seite(&a, &v["vergleich_id"]);
    let hinweise = |pfad: &str| zeilen.iter().find(|z| z["pfad"] == pfad).unwrap()["warnungen"].clone();
    assert_eq!(hinweise("index.php"), json!([]));
    assert_eq!(hinweise("gleich.css"), json!([{ "art": "inhalt_nicht_geprueft" }]));

    // Ersetzen: der Testserver überschreibt beim Umbenennen → gelernt „ja“
    std::fs::write(q.join("index.php"), "neuer!").unwrap();
    let (id, vorschau) = ftps_planen(&a, false);
    assert_eq!(vorschau["zusammenfassung"]["ueberschreiben"], 1);
    uebertragen(&a, &id, true, false).unwrap();
    assert_eq!(std::fs::read_to_string(s.wurzel().join("index.php")).unwrap(), "neuer!");
    assert_eq!(gelernt(&a).as_deref(), Some("ja"));

    // Verbindungstest zeigt das Gelernte der Stufe
    let mut ziel = ftps_ziel(s.port, Some(&s.fingerprint));
    ziel["konto_ref"] = json!("stellwerk:ftps-web:server");
    let t = a.aufruf("verbindung_testen", json!({ "ziel": ziel })).unwrap();
    assert_eq!(t["umbenennen_ueberschreibt"], true);
    assert_eq!(t["verzeichnis"], "/", "Ergebnis des Tests bleibt flach");
}

#[test]
fn ftps_server_ueberschreibt_beim_umbenennen_nicht() {
    let a = app();
    let aufbau = stellwerk_testserver::Aufbau { umbenennen_ueberschreibt_nicht: true, ..Default::default() };
    let s = stellwerk_testserver::starten(aufbau);
    ftps_strecke(&a, &s);
    passwort_hinterlegen(&a);
    std::fs::write(s.wurzel().join("index.php"), "alt!").unwrap();
    for (inhalt, erwartet) in [("neu-1", "nein"), ("neu--2", "nein")] {
        std::fs::write(a.wurzel.join("daten/quelle/index.php"), inhalt).unwrap();
        let (id, _) = ftps_planen(&a, true);
        let r = uebertragen(&a, &id, true, false).unwrap();
        assert_eq!(r["ergebnis"]["fehler"], json!([]), "{r}");
        assert_eq!(std::fs::read_to_string(s.wurzel().join("index.php")).unwrap(), inhalt);
        assert_eq!(gelernt(&a).as_deref(), Some(erwartet));
    }
    let mut ziel = ftps_ziel(s.port, Some(&s.fingerprint));
    ziel["konto_ref"] = json!("stellwerk:ftps-web:server");
    assert_eq!(a.aufruf("verbindung_testen", json!({ "ziel": ziel })).unwrap()["umbenennen_ueberschreibt"], false);
}

#[test]
fn ftps_lauf_wird_nicht_angelegt_ohne_passwort_oder_bei_geaendertem_ziel() {
    let a = app();
    let s = stellwerk_testserver::starten(Default::default());
    ftps_strecke(&a, &s);
    passwort_hinterlegen(&a);

    // Passwort inzwischen entfernt: Fehler vor dem Lauf, nichts angelegt
    let (id, _) = ftps_planen(&a, true);
    a.aufruf("passwort_entfernen", json!({ "konto": "stellwerk:ftps-web:server" })).unwrap();
    assert_eq!(uebertragen(&a, &id, true, false).unwrap_err()["kategorie"], "nicht_gefunden");
    assert_eq!(a.aufruf("laeufe", json!({ "strecke": "ftps-web", "stufe": "server", "limit": 5 })).unwrap(), json!([]));
    assert!(s.wurzel().join("alt.txt").exists());

    // Ziel nach dem Plan geändert (anderer Serverpfad): abgewiesen
    passwort_hinterlegen(&a);
    let (id, _) = ftps_planen(&a, true);
    let mut strecke = a.aufruf("strecke_laden", json!({ "id": "ftps-web" })).unwrap();
    strecke["stufen"][0]["ziel"]["pfad"] = json!("/anders");
    a.aufruf("strecke_speichern", json!({ "strecke": strecke })).unwrap();
    let e = uebertragen(&a, &id, true, false).unwrap_err();
    assert_eq!(e["kategorie"], "nicht_unterstuetzt");
    assert!(e["meldung"].as_str().unwrap().contains("neu vergleichen"));
    // Stufe inzwischen ganz entfernt
    let (id, _) = {
        strecke["stufen"][0]["ziel"]["pfad"] = json!("/");
        a.aufruf("strecke_speichern", json!({ "strecke": strecke })).unwrap();
        ftps_planen(&a, true)
    };
    strecke["stufen"][0]["id"] = json!("umbenannt");
    a.aufruf("strecke_speichern", json!({ "strecke": strecke })).unwrap();
    assert_eq!(uebertragen(&a, &id, true, false).unwrap_err()["kategorie"], "nicht_unterstuetzt");
    assert!(s.wurzel().join("alt.txt").exists());
}

#[test]
fn verbindung_testen_ohne_gelerntes() {
    let a = app();
    let s = stellwerk_testserver::starten(Default::default());
    let pw = stellwerk_testserver::PASSWORT;
    for (konto, status) in [("anderes-konto", None), ("stellwerk:x:y", None), ("stellwerk:x:y", Some("quatsch"))] {
        if let Some(w) = status {
            db(&a).status_setzen("x", "y", "umbenennen_ueberschreibt", w, 0).unwrap();
        }
        let mut ziel = ftps_ziel(s.port, Some(&s.fingerprint));
        ziel["konto_ref"] = json!(konto);
        let t = a.aufruf("verbindung_testen", json!({ "ziel": ziel, "passwort": pw })).unwrap();
        assert_eq!(t["umbenennen_ueberschreibt"], Value::Null, "{konto} {status:?}");
    }
}

#[test]
fn ftps_rechte_nach_stufen_einstellung_aendern_den_letzten_stand_nicht() {
    let a = app();
    let s = stellwerk_testserver::starten(stellwerk_testserver::Aufbau { site_chmod: true, ..Default::default() });
    ftps_strecke(&a, &s);
    passwort_hinterlegen(&a);
    let mut strecke = a.aufruf("strecke_laden", json!({ "id": "ftps-web" })).unwrap();
    strecke["stufen"][0]["rechte"] = json!({ "modus": "fest", "ordner": "755", "dateien": "640" });
    a.aufruf("strecke_speichern", json!({ "strecke": strecke })).unwrap();

    let (id, vorschau) = ftps_planen(&a, true);
    assert_eq!(vorschau["zusammenfassung"]["rechte"], 1, "{vorschau}");
    let r = uebertragen(&a, &id, true, false).unwrap();
    assert_eq!(r["ergebnis"]["fehler"], json!([]), "{r}");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let modus = std::fs::metadata(s.wurzel().join("index.php")).unwrap().permissions().mode() & 0o777;
        assert_eq!(modus, 0o640);
    }
    let pfade: Vec<_> = db(&a).manifest("ftps-web", "server").unwrap().into_iter().map(|e| e.pfad.as_str().to_string()).collect();
    assert_eq!(pfade, ["gleich.css", "index.php"]);
}

// --- Umschalten (v0.5) -------------------------------------------------------

/// Vergleichsdaten mit einer zu überschreibenden Datei; Stufe im angegebenen Modus.
fn mit_umschalten(a: &Testapp, modus: &str) {
    mit_vergleichsdaten(a);
    std::fs::write(a.wurzel.join("daten/spiegel/index.php"), "alt-alt").unwrap();
    let mut s = strecke_json(&a.wurzel);
    s["stufen"][0]["modus"] = json!(modus);
    a.aufruf("strecke_speichern", json!({ "strecke": s })).unwrap();
}

fn weiter(a: &Testapp, lauf: &Value, schritt: &str) -> Result<Value, Value> {
    a.aufruf("lauf_weiter", json!({ "laufId": lauf, "schritt": schritt }))
}

fn spiegel(a: &Testapp) -> std::collections::BTreeMap<String, String> {
    let z = a.wurzel.join("daten/spiegel");
    walkdir_dateien(&z)
}

fn walkdir_dateien(wurzel: &std::path::Path) -> std::collections::BTreeMap<String, String> {
    let mut m = std::collections::BTreeMap::new();
    let mut offen = vec![wurzel.to_path_buf()];
    while let Some(d) = offen.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            let rel = p.strip_prefix(wurzel).unwrap().display().to_string();
            if p.is_dir() {
                m.insert(rel + "/", String::new());
                offen.push(p);
            } else {
                m.insert(rel, std::fs::read_to_string(&p).unwrap());
            }
        }
    }
    m
}

fn journal_status(a: &Testapp, lauf: &Value) -> Vec<String> {
    let j = a.aufruf("lauf_journal", json!({ "laufId": lauf })).unwrap();
    j.as_array().unwrap().iter().map(|z| z["status"].as_str().unwrap().to_string()).collect()
}

#[test]
fn umschalten_automatisch_stellt_bereit_der_dialog_fuehrt_weiter() {
    let a = app();
    mit_umschalten(&a, "umschalten");
    let (id, vorschau) = vergleichen_und_planen(&a);
    assert_eq!(vorschau["modus"], "umschalten");
    // v0.6.1: Der Kern stellt nur bereit – Umschalten und Übernehmen löst der Dialog aus
    let r = uebertragen(&a, &id, true, false).unwrap();
    assert_eq!(r["phase"], "hochgeladen", "{r}");
    let lauf = r["lauf_id"].clone();
    assert_eq!(weiter(&a, &lauf, "umschalten").unwrap()["phase"], "umgeschaltet");
    assert_eq!(weiter(&a, &lauf, "uebernehmen").unwrap()["phase"], "fertig");
    assert_eq!(
        spiegel(&a),
        std::collections::BTreeMap::from([("css/".into(), String::new()), ("css/site.css".into(), "c".into()), ("index.php".into(), "neu".into())])
    );
    assert!(journal_status(&a, &lauf).iter().all(|s| s == "erledigt"), "{:?}", journal_status(&a, &lauf));
    let aktionen: Vec<String> = protokoll(&a, lauf.as_str().unwrap()).into_iter().map(|(akt, _)| akt).collect();
    assert_eq!(aktionen, ["lauf_gestartet", "hochgeladen", "umgeschaltet", "uebernommen"]);
    let l = a.aufruf("laeufe", json!({ "strecke": "mailwerk-web", "stufe": "spiegel", "limit": 1 })).unwrap();
    assert_eq!((l[0]["modus"].clone(), l[0]["phase"].clone()), (json!("umschalten"), json!("fertig")));
    assert_eq!(l[0]["ergebnis"]["uebernehmen"]["erledigt"], 2, "{}", l[0]["ergebnis"]);
    // Danach wieder frei
    starten(&a);
}

#[test]
fn umschalten_manuell_sperrt_die_stufe_bis_zum_rollback() {
    let a = app();
    mit_umschalten(&a, "umschalten_manuell");
    let vorher = spiegel(&a);
    let (id, _) = vergleichen_und_planen(&a);
    let r = uebertragen(&a, &id, true, false).unwrap();
    assert_eq!(r["phase"], "hochgeladen");
    let lauf = r["lauf_id"].clone();
    assert_eq!(spiegel(&a)["index.php"], "alt-alt", "Live unverändert");
    assert!(spiegel(&a).contains_key(&format!("index.php.dpl-{}", lauf.as_str().unwrap())));

    // Gesperrt: kein Vergleich, kein Plan
    let e = a.aufruf("vergleich_starten", json!({ "strecke": "mailwerk-web", "stufe": "spiegel", "fortschritt": KANAL })).unwrap_err();
    assert_eq!(e["kategorie"], "lauf_offen");
    assert!(e["meldung"].as_str().unwrap().contains("„Umschalten“"), "{e}");
    assert_eq!(a.aufruf("plan_erstellen", json!({ "vergleichId": id })).unwrap_err()["kategorie"], "lauf_offen");

    // Falsche Schritte
    for schritt in ["uebernehmen", "rollback"] {
        let e = weiter(&a, &lauf, schritt).unwrap_err();
        assert!(e["meldung"].as_str().unwrap().contains("nicht möglich"), "{e}");
    }
    let s = weiter(&a, &lauf, "umschalten").unwrap();
    assert_eq!((s["phase"].clone(), s["fehler"].clone()), (json!("umgeschaltet"), json!([])));
    assert_eq!(spiegel(&a)["index.php"], "neu");
    let e = a.aufruf("vergleich_starten", json!({ "strecke": "mailwerk-web", "stufe": "spiegel", "fortschritt": KANAL })).unwrap_err();
    assert!(e["meldung"].as_str().unwrap().contains("„Übernehmen“"), "{e}");
    for schritt in ["umschalten", "abbrechen"] {
        assert_eq!(weiter(&a, &lauf, schritt).unwrap_err()["kategorie"], "nicht_unterstuetzt");
    }

    let s = weiter(&a, &lauf, "rollback").unwrap();
    assert_eq!(s["phase"], "zurueckgerollt");
    assert_eq!(spiegel(&a), vorher);
    assert!(journal_status(&a, &lauf).iter().any(|st| st == "zurueckgerollt"));
    assert_eq!(weiter(&a, &lauf, "rollback").unwrap_err()["kategorie"], "nicht_unterstuetzt", "beendet");
    starten(&a);
}

#[test]
fn umschalten_manuell_uebernehmen_und_abbrechen() {
    let a = app();
    mit_umschalten(&a, "umschalten_manuell");
    let (id, _) = vergleichen_und_planen(&a);
    let lauf = uebertragen(&a, &id, true, false).unwrap()["lauf_id"].clone();
    weiter(&a, &lauf, "umschalten").unwrap();
    assert_eq!(weiter(&a, &lauf, "uebernehmen").unwrap()["phase"], "fertig");
    assert!(journal_status(&a, &lauf).iter().all(|s| s == "erledigt"));
    assert!(!spiegel(&a).keys().any(|k| k.contains(".bak-") || k.contains(".dpl-")));

    // Neuer Lauf, gleich abbrechen
    std::fs::write(a.wurzel.join("daten/quelle/neu2.txt"), "n").unwrap();
    let vorher = spiegel(&a);
    let (id, _) = vergleichen_und_planen(&a);
    let lauf = uebertragen(&a, &id, true, false).unwrap()["lauf_id"].clone();
    assert_eq!(weiter(&a, &lauf, "abbrechen").unwrap()["phase"], "abgebrochen");
    assert_eq!(spiegel(&a), vorher);

    assert_eq!(weiter(&a, &json!("gibtsnicht"), "abbrechen").unwrap_err()["kategorie"], "nicht_gefunden");
}

#[test]
fn umschalten_fehler_beim_hochladen_und_beim_umschalten() {
    let a = app();
    mit_umschalten(&a, "umschalten");
    // Hochladen scheitert an einer verschwundenen Quelle: nie umschalten, Lauf bleibt offen
    let (id, _) = vergleichen_und_planen(&a);
    std::fs::remove_file(a.wurzel.join("daten/quelle/css/site.css")).unwrap();
    let r = uebertragen(&a, &id, true, false).unwrap();
    assert_eq!(r["phase"], "hochgeladen");
    assert_eq!(r["ergebnis"]["fehler"].as_array().unwrap().len(), 1);
    let lauf = r["lauf_id"].clone();
    assert_eq!(spiegel(&a)["index.php"], "alt-alt");
    assert!(weiter(&a, &lauf, "umschalten").unwrap_err()["meldung"].as_str().unwrap().contains("nicht möglich"));
    weiter(&a, &lauf, "abbrechen").unwrap();

    // Umschalten scheitert: eine neue Datei ist inzwischen am Ziel – gestoppt, Rollback möglich
    std::fs::write(a.wurzel.join("daten/quelle/css/site.css"), "c").unwrap();
    let vorher = spiegel(&a);
    let (id, _) = vergleichen_und_planen(&a);
    std::fs::create_dir_all(a.wurzel.join("daten/spiegel/css")).unwrap();
    std::fs::write(a.wurzel.join("daten/spiegel/css/site.css"), "fremd").unwrap();
    let r = uebertragen(&a, &id, true, false).unwrap();
    assert_eq!(r["phase"], "hochgeladen", "{r}");
    let lauf = r["lauf_id"].clone();
    let s = weiter(&a, &lauf, "umschalten").unwrap();
    assert_eq!(s["phase"], "umgeschaltet", "{s}");
    assert_eq!(s["fehler"][0]["pfad"], "css/site.css", "{s}");
    assert!(weiter(&a, &lauf, "uebernehmen").unwrap_err()["meldung"].as_str().unwrap().contains("nicht möglich"));
    let s = weiter(&a, &lauf, "rollback").unwrap();
    assert_eq!(s["phase"], "zurueckgerollt");
    let mut erwartet = vorher;
    erwartet.insert("css/".into(), String::new());
    erwartet.insert("css/site.css".into(), "fremd".into());
    assert_eq!(spiegel(&a), erwartet, "die fremde Datei bleibt unberührt");
}

#[test]
fn umschalten_ziel_geaendert_oder_laeuft_schon() {
    let a = app();
    mit_umschalten(&a, "umschalten_manuell");
    let (id, _) = vergleichen_und_planen(&a);
    let lauf = uebertragen(&a, &id, true, false).unwrap()["lauf_id"].clone();
    // Gleichzeitig läuft schon etwas auf der Stufe
    let state = a.fenster.state::<AppState>();
    let _halter = state.laeufe.anmelden("mailwerk-web/spiegel").unwrap();
    assert_eq!(weiter(&a, &lauf, "umschalten").unwrap_err()["kategorie"], "vorhanden");
    state.laeufe.abmelden("mailwerk-web/spiegel");
    // Ziel der Stufe geändert
    let mut s = strecke_json(&a.wurzel);
    s["stufen"][0]["modus"] = json!("umschalten_manuell");
    let anderes = a.wurzel.join("daten/anderes");
    std::fs::create_dir_all(&anderes).unwrap();
    s["stufen"][0]["ziel"]["pfad"] = json!(anderes);
    a.aufruf("strecke_speichern", json!({ "strecke": s })).unwrap();
    let e = weiter(&a, &lauf, "umschalten").unwrap_err();
    assert!(e["meldung"].as_str().unwrap().contains("seit dem Hochladen geändert"), "{e}");
}

#[test]
fn umschalten_abbruch_beim_hochladen_raeumt_auf() {
    let a = app();
    mit_umschalten(&a, "umschalten");
    let block = vec![1u8; 1 << 20];
    let mut f = std::fs::File::create(a.wurzel.join("daten/quelle/gross.bin")).unwrap();
    for _ in 0..64 {
        std::io::Write::write_all(&mut f, &block).unwrap();
    }
    drop(f);
    let vorher = spiegel(&a);
    let (id, _) = vergleichen_und_planen(&a);
    let r = std::thread::scope(|s| {
        let (fenster, id) = (&a.fenster, &id);
        let lauf = s.spawn(move || {
            let args = json!({ "vergleichId": id, "loeschenBestaetigt": true, "beiFehlerStoppen": false, "fortschritt": KANAL });
            aufruf_im(fenster, "lauf_starten", args).unwrap()
        });
        while a.aufruf("lauf_abbrechen", json!({ "strecke": "mailwerk-web", "stufe": "spiegel" })).unwrap() != json!(true) {
            std::thread::yield_now();
        }
        lauf.join().unwrap()
    });
    assert_eq!(r["phase"], "abgebrochen", "{r}");
    assert_eq!(spiegel(&a), vorher);
}

#[test]
fn umschalten_ftps_letzter_bekannter_stand() {
    let a = app();
    let s = stellwerk_testserver::starten(Default::default());
    ftps_strecke(&a, &s);
    std::fs::create_dir_all(s.wurzel().join("weg/innen")).unwrap();
    std::fs::write(s.wurzel().join("weg/innen/x.txt"), "x").unwrap();
    let q = a.wurzel.join("daten/quelle");
    std::fs::create_dir_all(q.join("css")).unwrap();
    std::fs::write(q.join("css/site.css"), "body{}").unwrap();
    passwort_hinterlegen(&a);
    let mut st = a.aufruf("strecke_laden", json!({ "id": "ftps-web" })).unwrap();
    st["stufen"][0]["modus"] = json!("umschalten_manuell");
    a.aufruf("strecke_speichern", json!({ "strecke": st })).unwrap();
    let pfade = |a: &Testapp| db(a).manifest("ftps-web", "server").unwrap().into_iter().map(|e| e.pfad.to_string()).collect::<Vec<_>>();

    let (id, _) = ftps_planen(&a, true);
    let vorher = pfade(&a);
    let lauf = uebertragen(&a, &id, true, false).unwrap()["lauf_id"].clone();
    assert_eq!(pfade(&a), vorher, "Hochladen ändert den bekannten Stand nicht");
    weiter(&a, &lauf, "umschalten").unwrap();
    let nach = pfade(&a);
    assert!(nach.contains(&"css/site.css".to_string()) && nach.contains(&"css".to_string()) && !nach.contains(&"alt.txt".to_string()), "{nach:?}");
    let hash = db(&a).manifest("ftps-web", "server").unwrap().into_iter().find(|e| e.pfad.as_str() == "index.php").unwrap().hash;
    assert!(hash.is_some(), "Hash aus dem Hochladen");
    weiter(&a, &lauf, "rollback").unwrap();
    assert_eq!(pfade(&a), vorher, "Rollback setzt den bekannten Stand zurück");

    // Automatisch bis zum Übernehmen: gelöschte Ordner fallen heraus
    st["stufen"][0]["modus"] = json!("umschalten");
    a.aufruf("strecke_speichern", json!({ "strecke": st })).unwrap();
    let (id, _) = ftps_planen(&a, true);
    let r = uebertragen(&a, &id, true, false).unwrap();
    assert_eq!(r["phase"], "hochgeladen", "{r}");
    weiter(&a, &r["lauf_id"], "umschalten").unwrap();
    assert_eq!(weiter(&a, &r["lauf_id"], "uebernehmen").unwrap()["phase"], "fertig");
    let nach = pfade(&a);
    assert!(!nach.iter().any(|p| p.starts_with("weg")), "{nach:?}");
    assert_eq!(std::fs::read_to_string(s.wurzel().join("index.php")).unwrap(), "neu");
}

#[test]
fn lauf_aufgeben_markiert_unterbrochen_und_gibt_die_stufe_frei() {
    let a = app();
    mit_umschalten(&a, "umschalten_manuell");
    let (id, _) = vergleichen_und_planen(&a);
    let lauf = uebertragen(&a, &id, true, false).unwrap()["lauf_id"].clone();
    let vorher = spiegel(&a);
    assert_eq!(a.aufruf("lauf_aufgeben", json!({ "laufId": lauf })).unwrap(), Value::Null);
    assert_eq!(spiegel(&a), vorher, "am Ziel bleibt alles liegen");
    let l = a.aufruf("laeufe", json!({ "strecke": "mailwerk-web", "stufe": "spiegel", "limit": 1 })).unwrap();
    assert_eq!(l[0]["phase"], "abgebrochen");
    assert_eq!(l[0]["ergebnis"]["unterbrochen"], json!({ "grund": "aufgegeben", "phase": "hochgeladen" }));
    assert_eq!(protokoll(&a, lauf.as_str().unwrap()).last().unwrap().0, "unterbrochen");
    // Die Stufe ist frei; die Schattendatei zeigt der Vergleich als zu löschen
    starten(&a);

    let e = a.aufruf("lauf_aufgeben", json!({ "laufId": lauf })).unwrap_err();
    assert_eq!(e["kategorie"], "nicht_unterstuetzt", "{e}");
    assert_eq!(a.aufruf("lauf_aufgeben", json!({ "laufId": "gibtsnicht" })).unwrap_err()["kategorie"], "nicht_gefunden");
    assert!(a.aufruf("lauf_aufgeben", json!({})).is_err(), "ohne Kennung");
}

#[test]
fn offene_laeufe_gelten_nach_dem_neustart_als_unterbrochen() {
    let a = app();
    mit_umschalten(&a, "umschalten_manuell");
    let (id, _) = vergleichen_und_planen(&a);
    let lauf = uebertragen(&a, &id, true, false).unwrap()["lauf_id"].clone();
    weiter(&a, &lauf, "umschalten").unwrap();
    let e = a.aufruf("vergleich_starten", json!({ "strecke": "mailwerk-web", "stufe": "spiegel", "fortschritt": KANAL })).unwrap_err();
    assert_eq!(e["kategorie"], "lauf_offen");

    // „Neustart“: ein neuer Zustand über demselben Konfigurationsordner
    let neu = AppState::new(ConfigStore::new(&a.wurzel), Box::new(SpeicherTresor::default())).unwrap();
    let gefunden = neu.datenbank.lock().unwrap().lauf(lauf.as_str().unwrap()).unwrap().unwrap();
    assert_eq!(gefunden.phase, stellwerk_core::store::LaufPhase::Abgebrochen);
    assert_eq!(gefunden.ergebnis.unwrap()["unterbrochen"], json!({ "grund": "programm_beendet", "phase": "umgeschaltet" }));
    assert!(neu.datenbank.lock().unwrap().offene_laeufe().unwrap().is_empty());
    // Live ist der neue Stand, die Sicherung liegt noch da – der Vergleich zeigt sie
    assert_eq!(spiegel(&a)["index.php"], "neu");
    assert!(spiegel(&a).contains_key(&format!("index.php.bak-{}", lauf.as_str().unwrap())));
    starten(&a);
}

#[test]
fn lauf_weiter_mit_unbekanntem_schritt_wird_abgewiesen() {
    let a = app();
    assert!(a.aufruf("lauf_weiter", json!({ "laufId": "x", "schritt": 5 })).is_err());
}

// --- Inhalte prüfen (v0.5) -----------------------------------------------------

#[test]
fn inhalte_pruefen_macht_den_inhalt_bekannt() {
    let a = app();
    let s = stellwerk_testserver::starten(Default::default());
    ftps_strecke(&a, &s);
    std::fs::write(s.wurzel().join("gleich2.css"), "aaa").unwrap();
    std::fs::write(a.wurzel.join("daten/quelle/gleich2.css"), "bbb").unwrap();
    passwort_hinterlegen(&a);
    let v = ftps_vergleich(&a, Some(true)).unwrap();
    assert_eq!((v["ungeprueft"]["anzahl"].clone(), v["ungeprueft"]["bytes"].clone()), (json!(2), json!(6)), "{v}");
    let id = v["vergleich_id"].clone();
    // Eine der beiden verschwindet vor dem Prüfen vom Server
    std::fs::remove_file(s.wurzel().join("gleich2.css")).unwrap();
    let r = a.aufruf("inhalte_pruefen", json!({ "vergleichId": id, "fortschritt": KANAL })).unwrap();
    assert_eq!(r["geprueft"], 1, "{r}");
    assert_eq!(r["fehler"][0]["pfad"], "gleich2.css");
    assert_eq!(std::fs::read_to_string(s.wurzel().join("gleich.css")).unwrap(), "xyz", "am Server nichts verändert");

    // Ohne Serverkontakt neu vergleichen: der Inhalt ist jetzt bekannt – und verschieden
    let v = ftps_vergleich(&a, Some(false)).unwrap();
    assert_eq!(v["ungeprueft"]["anzahl"], 1, "nur noch die nicht geprüfte");
    let zeile = seite(&a, &v["vergleich_id"]).into_iter().find(|z| z["pfad"] == "gleich.css").unwrap();
    assert_eq!(zeile["status"], "geaendert", "{zeile}");
}

#[test]
fn inhalte_pruefen_nur_fuer_ftps() {
    let a = app();
    mit_vergleichsdaten(&a);
    let id = starten(&a)["vergleich_id"].clone();
    let e = a.aufruf("inhalte_pruefen", json!({ "vergleichId": id, "fortschritt": KANAL })).unwrap_err();
    assert_eq!(e["kategorie"], "nicht_unterstuetzt");
    assert_eq!(a.aufruf("inhalte_pruefen", json!({ "vergleichId": "x", "fortschritt": KANAL })).unwrap_err()["kategorie"], "nicht_gefunden");
    assert!(a.aufruf("inhalte_pruefen", json!({ "vergleichId": id })).is_err(), "ohne Kanal");
}
