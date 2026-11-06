import { describe, expect, it } from "vitest";

import type { Strecke } from "../api";
import {
  endpunktVom,
  freieStufenId,
  hatFehler,
  kennungAus,
  kontenAngleichen,
  kontoRef,
  neueRegel,
  neueStrecke,
  neueStufe,
  problemeAn,
  quelleVom,
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
  });

  it("kontenAngleichen setzt den internen Schlüsselbund-Namen jeder FTPS-Stufe", () => {
    const lokal = neueStufe(leer);
    const ftps = { ...neueStufe(leer), id: "server", ziel: endpunktVom("ftps", "falsch-oder-passwort") };
    const s = kontenAngleichen({ ...leer, id: "web", stufen: [lokal, ftps] });
    expect(s.stufen[0]).toBe(lokal);
    expect(s.stufen[1].ziel).toMatchObject({ konto_ref: "stellwerk:web:server" });
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
