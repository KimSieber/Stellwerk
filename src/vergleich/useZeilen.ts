// Lädt die sichtbaren Zeilen eines Vergleichs seitenweise von der Rust-Seite
// (`diff_seite`). Filter und zugeklappte Ordner wertet Rust aus.

import { useCallback, useEffect, useRef, useState } from "react";

import { api, fehlerText, type Filter, type Zeile } from "../api";

/** Zeilen je Abruf. */
export const SEITE = 200;

type Stand = {
  /** Abfrage, zu der die Daten gehören (Filter, Zugeklapptes, Auswahlstand). */
  schluessel: string;
  /** Anzahl sichtbarer Zeilen; `null`, solange noch nichts geladen ist. */
  sichtbar: number | null;
  seiten: Map<number, Zeile[]>;
  fehler: string | null;
};

/**
 * `stand` zählt hoch, wenn sich die Auswahl geändert hat. Bis die neuen Seiten
 * da sind, bleiben die bisherigen sichtbar (kein Flackern).
 */
export function useZeilen(vergleichId: string, filter: Filter, zugeklappt: string[], stand: number) {
  const schluessel = JSON.stringify([vergleichId, filter, zugeklappt, stand]);
  const aktuell = useRef(schluessel);
  aktuell.current = schluessel;
  const unterwegs = useRef(new Set<string>());
  const [daten, setDaten] = useState<Stand>({ schluessel, sichtbar: null, seiten: new Map(), fehler: null });

  const laden = useCallback(
    (seite: number) => {
      const marke = `${schluessel}|${seite}`;
      if (unterwegs.current.has(marke)) return;
      unterwegs.current.add(marke);
      api
        .diffSeite(vergleichId, seite * SEITE, SEITE, filter, zugeklappt)
        .then((s) => {
          unterwegs.current.delete(marke);
          if (aktuell.current !== schluessel) return; // veraltete Antwort
          setDaten((d) => {
            // Erste Antwort einer neuen Abfrage ersetzt alle alten Seiten
            const seiten = d.schluessel === schluessel ? new Map(d.seiten) : new Map<number, Zeile[]>();
            seiten.set(seite, s.zeilen);
            return { schluessel, sichtbar: s.sichtbar, seiten, fehler: null };
          });
        })
        .catch((e) => {
          unterwegs.current.delete(marke);
          setDaten((d) => ({ ...d, fehler: fehlerText(e) }));
        });
    },
    // `schluessel` enthält alle übrigen Abhängigkeiten
    [schluessel],
  );

  // Neue Abfrage: erste Seite sofort holen
  useEffect(() => laden(0), [laden]);

  /** Zeile `i`, sofern ihre Seite geladen ist – sonst wird sie angefordert. */
  const zeile = useCallback(
    (i: number): Zeile | undefined => {
      const seite = Math.floor(i / SEITE);
      if (daten.schluessel !== schluessel || !daten.seiten.has(seite)) laden(seite);
      return daten.seiten.get(seite)?.[i % SEITE];
    },
    [daten, schluessel, laden],
  );

  return { sichtbar: daten.sichtbar, fehler: daten.fehler, zeile };
}
