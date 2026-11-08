// Attrappe für das App-Fenster (v0.6.1): ersetzt getCurrentWindow() in allen Tests
// (siehe setup.ts). Tests lösen „Fenster schließen“ mit schliessenAnfordern() aus.

import { vi } from "vitest";

type Schliessen = (e: { preventDefault: () => void }) => void;

const empfaenger: Schliessen[] = [];

/** Wurde das Fenster (endgültig) geschlossen? */
export const fensterZu = vi.fn();

export function appFensterAttrappe() {
  return {
    onCloseRequested(cb: Schliessen) {
      empfaenger.push(cb);
      return Promise.resolve(() => {
        empfaenger.splice(empfaenger.indexOf(cb), 1);
      });
    },
    destroy: () => Promise.resolve(fensterZu()),
  };
}

/** Wie der Schließknopf des Fensters; liefert, ob das Schließen verhindert wurde. */
export function schliessenAnfordern(): boolean {
  let verhindert = false;
  for (const e of [...empfaenger]) e({ preventDefault: () => (verhindert = true) });
  return verhindert;
}
