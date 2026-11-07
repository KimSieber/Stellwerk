import { describe, expect, it } from "vitest";
import { Channel } from "@tauri-apps/api/core";

import { api, fehlerText, type Fortschritt, type Strecke } from "./api";
import { rustAttrappe } from "./test/hilfen";

/** Interne Rückrufe der Tauri-Attrappe (nicht typisiert in @tauri-apps/api). */
function tauriIntern() {
  return (window as unknown as { __TAURI_INTERNALS__: { runCallback: (id: number, daten: unknown) => void } })
    .__TAURI_INTERNALS__;
}

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

  it("vergleichStarten übergibt einen Kanal, dessen Meldungen ankommen", async () => {
    const start = { vergleich_id: "s/a#1", quelle: "/q", ziel: "/z", zeilen: 0, lesefehler: [] };
    const aufrufe = rustAttrappe({
      vergleich_starten: (args) => {
        const kanal = (args as { fortschritt: Channel<Fortschritt> }).fortschritt;
        // so liefert Rust: Index + Nachricht über den registrierten Rückruf
        tauriIntern().runCallback(kanal.id, { index: 0, message: { phase: "ziel", anzahl: 7 } });
        return start;
      },
    });
    const meldungen: Fortschritt[] = [];
    await expect(api.vergleichStarten("s", "a", (f) => meldungen.push(f))).resolves.toEqual(start);
    expect(meldungen).toEqual([{ phase: "ziel", anzahl: 7 }]);
    expect(aufrufe[0].cmd).toBe("vergleich_starten");
    expect(aufrufe[0].args).toMatchObject({ strecke: "s", stufe: "a" });
    expect((aufrufe[0].args as { fortschritt: unknown }).fortschritt).toBeInstanceOf(Channel);
  });

  it("vergleichAbbrechen", async () => {
    const aufrufe = rustAttrappe({ vergleich_abbrechen: () => true });
    await expect(api.vergleichAbbrechen("s", "a")).resolves.toBe(true);
    expect(aufrufe).toEqual([{ cmd: "vergleich_abbrechen", args: { strecke: "s", stufe: "a" } }]);
  });

  it("diffSeite", async () => {
    const aufrufe = rustAttrappe({ diff_seite: () => ({ zeilen: [], sichtbar: 0 }) });
    await expect(api.diffSeite("v", 10, 50, "alle", ["css"])).resolves.toEqual({ zeilen: [], sichtbar: 0 });
    expect(aufrufe).toEqual([
      { cmd: "diff_seite", args: { vergleichId: "v", offset: 10, limit: 50, filter: "alle", zugeklappt: ["css"] } },
    ]);
  });

  it("auswahlSetzen", async () => {
    const aufrufe = rustAttrappe({ auswahl_setzen: () => ({ ausgewaehlt: 1 }) });
    await expect(api.auswahlSetzen("v", ["a"], false)).resolves.toEqual({ ausgewaehlt: 1 });
    expect(aufrufe).toEqual([{ cmd: "auswahl_setzen", args: { vergleichId: "v", pfade: ["a"], an: false } }]);
  });

  it("vergleichSchliessen", async () => {
    const aufrufe = rustAttrappe({ vergleich_schliessen: () => null });
    await api.vergleichSchliessen("v");
    expect(aufrufe).toEqual([{ cmd: "vergleich_schliessen", args: { vergleichId: "v" } }]);
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
