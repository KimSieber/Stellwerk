import { describe, expect, it } from "vitest";

import type { Strecke } from "../api";
import {
  endpunktVom,
  freieStufenId,
  gruppeBereinigt,
  hatFehler,
  kennungAus,
  ausUmschalten,
  kontenAngleichen,
  musterAnzeige,
  ortLesbar,
  regelnFuerStrecke,
  regelnFuerStufe,
  pfadSetzen,
  kontoRef,
  neueRegel,
  neueStrecke,
  neueStufe,
  problemeAn,
  quelleVom,
  mitPasswort,
  smbAdresse,
  smbAusAdresse,
} from "./modell";

describe("kennungAus", () => {
  it.each([
    ["Mailwerk Webseite", "mailwerk-webseite"],
    ["Größe & Übersicht", "groesse-uebersicht"],
    ["Straße ÄÖÜ", "strasse-aeoeue"],
    ["Café Crème", "cafe-creme"],
    ["  --a--  ", "a"],
    ["PictureSearchView → KI", "picturesearchview-ki"],
    ["", ""],
  ])("%s → %s", (name, kennung) => {
    expect(kennungAus(name)).toBe(kennung);
  });

  it("höchstens 64 Zeichen und ohne Strich am Ende", () => {
    expect(kennungAus("a".repeat(63) + " b")).toBe("a".repeat(63));
    expect(kennungAus("x".repeat(100))).toHaveLength(64);
  });
});

describe("gruppeBereinigt", () => {
  it("entfernt Rand-Leerzeichen; leer heißt ohne Gruppe (Feld entfällt)", () => {
    const s = neueStrecke();
    expect(gruppeBereinigt({ ...s, gruppe: "  Webseiten " }).gruppe).toBe("Webseiten");
    expect(gruppeBereinigt({ ...s, gruppe: "  " })).not.toHaveProperty("gruppe");
    expect(gruppeBereinigt(s)).toEqual(s);
  });
});

describe("Stufen und Strecken", () => {
  const leer: Strecke = { id: "s", name: "S", version: 1, stufen: [] };

  it("kontoRef", () => {
    expect(kontoRef("mailwerk-web", "server")).toBe("stellwerk:mailwerk-web:server");
  });

  it("freieStufenId überspringt vergebene Kennungen", () => {
    expect(freieStufenId(leer)).toBe("stufe-1");
    const s = { ...leer, stufen: [neueStufe(leer)] };
    s.stufen[0].id = "stufe-2";
    expect(freieStufenId(s)).toBe("stufe-3");
  });

  it("neue Stufe übernimmt den Zielordner der Vorstufe als lokale Quelle", () => {
    const erste = neueStufe(leer);
    expect(erste).toMatchObject({ id: "stufe-1", name: "Stufe 1", modus: "direkt", nur_online: "herunterladen" });
    expect(erste.quelle).toEqual({ typ: "lokal", pfad: "" });

    const mitSpiegel = { ...erste, ziel: { typ: "lokal" as const, pfad: "/Users/kim/Server/mailwerk" } };
    const zweite = neueStufe({ ...leer, stufen: [mitSpiegel] });
    expect(zweite.quelle).toEqual({ typ: "lokal", pfad: "/Users/kim/Server/mailwerk" });
    expect(zweite.name).toBe("Stufe 2");
  });

  it("nach einem FTPS-Ziel bleibt die Quelle leer", () => {
    const ftps = { ...neueStufe(leer), ziel: endpunktVom("ftps", "k") };
    expect(neueStufe({ ...leer, stufen: [ftps] }).quelle).toEqual({ typ: "lokal", pfad: "" });
  });

  it("neue Strecke hat eine Stufe", () => {
    const s = neueStrecke();
    expect(s).toMatchObject({ id: "", name: "", version: 1 });
    expect(s.stufen).toHaveLength(1);
  });

  it("Endpunkte und Quellen je Typ", () => {
    expect(endpunktVom("lokal", "k")).toEqual({ typ: "lokal", pfad: "" });
    expect(endpunktVom("ftps", "k")).toEqual({
      typ: "ftps", host: "", port: 21, pfad: "/", benutzer: "", tls: "explizit", konto_ref: "k",
    });
    expect(quelleVom("stufe", "vor")).toEqual({ typ: "stufe", stufe: "vor" });
    expect(quelleVom("stufe")).toEqual({ typ: "stufe", stufe: "" });
    expect(quelleVom("lokal")).toEqual({ typ: "lokal", pfad: "" });
    expect(quelleVom("ftps", undefined, "k")).toEqual(endpunktVom("ftps", "k"));
  });

  it("kontenAngleichen setzt den internen Schlüsselbund-Namen jeder FTPS-Stufe", () => {
    const lokal = neueStufe(leer);
    const ftps = { ...neueStufe(leer), id: "server", ziel: endpunktVom("ftps", "falsch-oder-passwort") };
    const s = kontenAngleichen({ ...leer, id: "web", stufen: [lokal, ftps] });
    expect(s.stufen[0]).toBe(lokal);
    expect(s.stufen[1].ziel).toMatchObject({ konto_ref: "stellwerk:web:server" });
    // v0.6.5: auch eine Quelle auf dem Server – mit eigenem Namen
    const holen = { ...neueStufe(leer), id: "holen", quelle: endpunktVom("ftps", "x") };
    const t = kontenAngleichen({ ...leer, id: "web", stufen: [holen] });
    expect(t.stufen[0].quelle).toMatchObject({ konto_ref: "stellwerk:web:holen:quelle" });
  });

  it("neue Regel", () => {
    expect(neueRegel()).toEqual({ muster: "", typ: "ignorieren" });
  });
});

describe("Prüfmeldungen", () => {
  const p = [
    { schwere: "fehler" as const, ort: "name", meldung: "Name fehlt" },
    { schwere: "warnung" as const, ort: "stufen", meldung: "keine Stufe" },
  ];
  it("problemeAn filtert genau nach Ort", () => {
    expect(problemeAn(p, "name")).toEqual([p[0]]);
    expect(problemeAn(p, "nam")).toEqual([]);
  });
  it("hatFehler", () => {
    expect(hatFehler(p)).toBe(true);
    expect(hatFehler([p[1]])).toBe(false);
  });
});

describe("pfadSetzen (Ablage per Drag-and-drop)", () => {
  const s: Strecke = {
    id: "s",
    name: "S",
    version: 1,
    stufen: [
      { ...neueStufe({ id: "", name: "", version: 1, stufen: [] }), ziel: endpunktVom("ftps", "k") },
    ],
  };
  it("setzt Quelle oder Ziel einer Stufe", () => {
    expect(pfadSetzen(s, "stufen[0].quelle.pfad", "/a").stufen[0].quelle).toEqual({ typ: "lokal", pfad: "/a" });
  });
  it("ignoriert FTPS-Ziele, fremde Orte und fehlende Stufen", () => {
    expect(pfadSetzen(s, "stufen[0].ziel.pfad", "/a")).toBe(s);
    expect(pfadSetzen(s, "name", "/a")).toBe(s);
    expect(pfadSetzen(s, "stufen[5].quelle.pfad", "/a")).toBe(s);
  });
});

describe("ortLesbar", () => {
  const basis = neueStufe({ id: "", name: "", version: 1, stufen: [] });
  const s = {
    id: "s",
    name: "S",
    version: 1,
    stufen: [{ ...basis, name: "Server" }, { ...basis, name: "", ziel: endpunktVom("ftps", "k") }],
  };
  it.each([
    ["name", "Name"],
    ["id", "Kennung"],
    ["stufen", "Stufe"],
    ["stufen[0].ziel.pfad", "Stufe 1 „Server“ › Ziel › Ordner"],
    ["stufen[0].quelle.pfad", "Stufe 1 „Server“ › Quelle › Ordner"],
    ["stufen[1].ziel.pfad", "Stufe 2 › Ziel › Serverpfad"],
    ["stufen[5].ziel.pfad", "Stufe 6 › Ziel › Pfad"],
    ["stufen[3].quelle.typ", "Stufe 4 › Quelle › Art"],
    ["stufen[0].regeln.eigene[2].muster", "Stufe 1 „Server“ › Regeln › Regel 3 › Muster"],
    ["regeln.eigene[0].quelle", "Regeln der Strecke › Regel 1 › Quelle"],
    ["regeln.standard_aus[1]", "Regeln der Strecke › Abschaltung 2"],
    ["stufen[0].rechte.regeln[0].muster", "Stufe 1 „Server“ › Dateirechte › Regeln 1 › Muster"],
    ["unbekannt.feld", "unbekannt › feld"],
  ])("%s → %s", (ort, text) => {
    expect(ortLesbar(ort, s as Strecke)).toBe(text);
  });
  it("ohne Strecke nur mit Nummer", () => {
    expect(ortLesbar("stufen[0].name")).toBe("Stufe 1 › Name");
    expect(ortLesbar("stufen[0].ziel.pfad")).toBe("Stufe 1 › Ziel › Pfad");
  });
});

describe("Übernommene Regeln", () => {
  const global = [
    { muster: ".DS_Store", typ: "ignorieren" as const },
    { muster: ".git/", typ: "ignorieren" as const },
    { muster: "*.alt", typ: "ignorieren" as const, aktiv: false },
  ];
  const s: Strecke = {
    id: "s",
    name: "S",
    version: 1,
    regeln: {
      standard_aus: [".git/"],
      eigene: [
        { muster: "logs/", typ: "schuetzen" },
        { muster: "aus/", typ: "ignorieren", aktiv: false },
        { muster: " ", typ: "ignorieren" },
      ],
    },
    stufen: [],
  };

  it("Strecke: nur wirksame globale Regeln", () => {
    expect(regelnFuerStrecke(global)).toEqual([
      { muster: ".DS_Store", typ: "ignorieren", herkunft: "global" },
      { muster: ".git/", typ: "ignorieren", herkunft: "global" },
    ]);
  });

  it("Stufe: globale ohne die von der Strecke abgeschalteten, plus wirksame Strecken-Regeln", () => {
    expect(regelnFuerStufe(global, s)).toEqual([
      { muster: ".DS_Store", typ: "ignorieren", herkunft: "global" },
      { muster: "logs/", typ: "schuetzen", herkunft: "strecke" },
    ]);
    expect(regelnFuerStufe(global, { ...s, regeln: undefined })).toHaveLength(2);
  });

  it("ausUmschalten ohne Doppelungen", () => {
    expect(ausUmschalten([], "a", false)).toEqual(["a"]);
    expect(ausUmschalten(["a"], "a", false)).toEqual(["a"]);
    expect(ausUmschalten(["a", "b"], "a", true)).toEqual(["b"]);
  });

  it("musterAnzeige", () => {
    expect(musterAnzeige("Icon[\r]")).toBe("Icon[\\r]");
    expect(musterAnzeige("*.log")).toBe("*.log");
  });
});

describe("Samba-Freigabe (v0.7.1)", () => {
  const leer = { host: "", port: 445, freigabe: "", pfad: "/", benutzer: "b", konto_ref: "k" };

  it("Adresse aus den Feldern", () => {
    expect(smbAdresse(leer)).toBe("");
    expect(smbAdresse({ ...leer, host: "pi" })).toBe("smb://pi/");
    expect(smbAdresse({ ...leer, freigabe: "f" })).toBe("smb:///f");
    expect(smbAdresse({ ...leer, host: "pi", freigabe: "f", pfad: "/a/b/" })).toBe("smb://pi/f/a/b");
    expect(smbAdresse({ ...leer, host: "pi", port: 4445, freigabe: "f" })).toBe("smb://pi:4445/f");
  });

  it("Felder aus der Adresse – auch ohne Präfix und aus dem Finder", () => {
    expect(smbAusAdresse("smb://192.168.1.11/manitu", leer)).toEqual({ ...leer, host: "192.168.1.11", freigabe: "manitu" });
    expect(smbAusAdresse(" SMB://pi:4445/f/a//b/ ", leer)).toEqual({ ...leer, host: "pi", port: 4445, freigabe: "f", pfad: "/a/b" });
    expect(smbAusAdresse("pi/Meine%20Daten", leer)).toMatchObject({ host: "pi", freigabe: "Meine Daten" });
    expect(smbAusAdresse("pi:/f", leer).port).toBe(0);
    expect(smbAusAdresse("smb://pi/f%2", leer).freigabe).toBe("f%2"); // beim Tippen unvollständig
    expect(smbAusAdresse("", leer)).toEqual({ ...leer, host: "", freigabe: "", pfad: "/" });
  });

  it("Passwort, Konten und Feldnamen", () => {
    const smb = { typ: "smb" as const, ...leer };
    expect(mitPasswort(smb)).toBe(true);
    expect(mitPasswort({ typ: "lokal", pfad: "" })).toBe(false);
    expect(endpunktVom("smb", "k2")).toEqual({ typ: "smb", host: "", port: 445, freigabe: "", pfad: "/", benutzer: "", konto_ref: "k2" });
    const s: Strecke = {
      id: "s", name: "S", version: 1,
      stufen: [{ id: "a", name: "A", modus: "direkt", quelle: smb, ziel: smb, nur_online: "herunterladen" }],
    };
    const k = kontenAngleichen(s).stufen[0];
    expect([k.ziel, k.quelle].map((e) => (e as typeof smb).konto_ref)).toEqual(["stellwerk:s:a", "stellwerk:s:a:quelle"]);
    expect(ortLesbar("stufen[0].ziel.pfad", s)).toBe("Stufe 1 „A“ › Ziel › Ordner in der Freigabe");
    expect(ortLesbar("stufen[0].ziel.freigabe", s)).toBe("Stufe 1 „A“ › Ziel › Freigabe");
  });
});

describe("SFTP-Server (v0.7.2)", () => {
  it("leer mit Port 22, mit Passwort, eigene Konten, Ortsnamen", () => {
    const sftp = endpunktVom("sftp", "k");
    expect(sftp).toEqual({ typ: "sftp", host: "", port: 22, pfad: "/", benutzer: "", konto_ref: "k" });
    expect(mitPasswort(sftp)).toBe(true);
    const s = kontenAngleichen({
      id: "s", name: "S", version: 1,
      stufen: [{ id: "a", name: "A", modus: "direkt", quelle: sftp, ziel: sftp, nur_online: "herunterladen" }],
    });
    expect([s.stufen[0].ziel, s.stufen[0].quelle].map((e) => (e as typeof sftp & { konto_ref: string }).konto_ref)).toEqual([
      "stellwerk:s:a",
      "stellwerk:s:a:quelle",
    ]);
    expect(ortLesbar("stufen[0].ziel.pfad", s)).toBe("Stufe 1 „A“ › Ziel › Serverpfad");
    expect(ortLesbar("stufen[0].ziel.hostschluessel", s)).toBe("Stufe 1 „A“ › Ziel › Host-Schlüssel");
    expect(ortLesbar("stufen[0].ziel.schluessel", s)).toBe("Stufe 1 „A“ › Ziel › SSH-Schlüssel");
  });
});
