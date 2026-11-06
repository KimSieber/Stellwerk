import { describe, expect, it } from "vitest";

import { api, fehlerText, type Strecke } from "./api";
import { rustAttrappe } from "./test/hilfen";

const strecke: Strecke = {
  id: "s",
  name: "S",
  version: 1,
  stufen: [],
};

describe("api – jeder Befehl mit richtigem Namen und Parametern", () => {
  it("konfigOrt", async () => {
    const aufrufe = rustAttrappe({ konfig_ort: () => "/pfad" });
    await expect(api.konfigOrt()).resolves.toBe("/pfad");
    expect(aufrufe).toEqual([{ cmd: "konfig_ort", args: {} }]);
  });

  it("streckenListe", async () => {
    const aufrufe = rustAttrappe({ strecken_liste: () => [] });
    await expect(api.streckenListe()).resolves.toEqual([]);
    expect(aufrufe[0].cmd).toBe("strecken_liste");
  });

  it("streckeLaden", async () => {
    const aufrufe = rustAttrappe({ strecke_laden: () => strecke });
    await expect(api.streckeLaden("s")).resolves.toEqual(strecke);
    expect(aufrufe).toEqual([{ cmd: "strecke_laden", args: { id: "s" } }]);
  });

  it("streckePruefen", async () => {
    const aufrufe = rustAttrappe({ strecke_pruefen: () => [] });
    await expect(api.streckePruefen(strecke)).resolves.toEqual([]);
    expect(aufrufe).toEqual([{ cmd: "strecke_pruefen", args: { strecke } }]);
  });

  it("streckeSpeichern", async () => {
    const aufrufe = rustAttrappe({ strecke_speichern: () => [] });
    await expect(api.streckeSpeichern(strecke)).resolves.toEqual([]);
    expect(aufrufe).toEqual([{ cmd: "strecke_speichern", args: { strecke } }]);
  });

  it("streckeLoeschen", async () => {
    const aufrufe = rustAttrappe({ strecke_loeschen: () => null });
    await api.streckeLoeschen("s");
    expect(aufrufe).toEqual([{ cmd: "strecke_loeschen", args: { id: "s" } }]);
  });

  it("Fehler der Rust-Seite kommen als Ablehnung an", async () => {
    rustAttrappe({
      strecke_laden: () => {
        throw { kategorie: "nicht_gefunden", meldung: "Nicht gefunden: Strecke 'x'" };
      },
    });
    await expect(api.streckeLaden("x")).rejects.toMatchObject({ kategorie: "nicht_gefunden" });
  });
});

describe("fehlerText", () => {
  it("nimmt die Meldung eines Befehlsfehlers", () => {
    expect(fehlerText({ kategorie: "io", meldung: "kaputt" })).toBe("kaputt");
  });

  it("wandelt alles andere in Text", () => {
    expect(fehlerText("einfach")).toBe("einfach");
    expect(fehlerText(new Error("Fehler"))).toBe("Error: Fehler");
    expect(fehlerText(null)).toBe("null");
    expect(fehlerText({ anders: 1 })).toBe("[object Object]");
  });
});
