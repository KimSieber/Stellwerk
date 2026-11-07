// Testhilfen für die Vergleichsansicht: Beispielzeilen und eine Nachbildung
// der Rust-Befehle (`diff_seite`, `auswahl_setzen`) – stark vereinfacht.
import { vi } from "vitest";

import type { Eintrag, Seite, VergleichStart, Zeile, Zusammenfassung } from "../api";

export function eintrag(pfad: string, art: Eintrag["art"] = "datei", teil: Partial<Eintrag> = {}): Eintrag {
  return { pfad, art, groesse: 1500, mtime_ns: 1_700_000_000_000_000_000, nur_online: false, hash: null, warnungen: [], ...teil };
}

export function zeile(pfad: string, teil: Partial<Zeile> = {}): Zeile {
  const name = pfad.split("/").pop()!;
  return {
    pfad,
    status: "neu",
    quelle: eintrag(pfad),
    ziel: null,
    regel: null,
    warnungen: [],
    ausgewaehlt: true,
    name,
    tiefe: pfad.split("/").length - 1,
    ordner: false,
    kinder: false,
    zugeklappt: false,
    auswaehlbar: true,
    regel_entfernbar: false,
    ...teil,
  };
}

export function summe(teil: Partial<Zusammenfassung> = {}): Zusammenfassung {
  return {
    neu: 0, geaendert: 0, loeschen: 0, gleich: 0, geschuetzt: 0, ignoriert: 0, konflikt: 0,
    ausgewaehlt: 0, bytes_uebertragen: 0, nur_online: 0, nur_online_bytes: 0, ...teil,
  };
}

export function start(teil: Partial<VergleichStart> = {}): VergleichStart {
  return {
    vergleich_id: "web/spiegel#1",
    quelle: "/Users/kim/Dev/web",
    ziel: "/Users/kim/Server/web",
    zusammenfassung: summe({ neu: 2, ausgewaehlt: 2, bytes_uebertragen: 3000 }),
    zeilen: 2,
    lesefehler: [],
    ...teil,
  };
}

/** Nachbildung von `diff_seite`: zugeklappte Ordner verbergen ihren Inhalt. */
export function seiteAus(alle: Zeile[]) {
  return (args: unknown): Seite => {
    const { offset, limit, zugeklappt } = args as { offset: number; limit: number; zugeklappt: string[] };
    const sichtbar = alle
      .filter((z) => !zugeklappt.some((p) => z.pfad.startsWith(`${p}/`)))
      .map((z) => ({ ...z, zugeklappt: zugeklappt.includes(z.pfad) }));
    return { zeilen: sichtbar.slice(offset, offset + limit), sichtbar: sichtbar.length };
  };
}

/** Gibt Elementen eine Größe, damit die virtuelle Liste in jsdom Zeilen zeigt. */
export function listenGroesse(hoehe = 600) {
  vi.spyOn(Element.prototype, "clientHeight", "get").mockReturnValue(hoehe);
  vi.spyOn(Element.prototype, "clientWidth", "get").mockReturnValue(900);
}

/** Schickt eine Nachricht über einen Tauri-Kanal, so wie Rust es tut. */
export function kanalSenden(kanal: { id: number }, index: number, nachricht: unknown) {
  (window as unknown as { __TAURI_INTERNALS__: { runCallback: (id: number, d: unknown) => void } }).__TAURI_INTERNALS__.runCallback(
    kanal.id,
    { index, message: nachricht },
  );
}
