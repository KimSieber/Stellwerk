// Gemeinsame Helfer für `npm run app` und `npm run dmg`: Befehle starten, Ausgabe zeigen,
// bei Fehlern abbrechen. Nur Ablauf – die Entscheidungen stecken in app-logik.mjs (getestet).

import { execFileSync, spawnSync } from "node:child_process";

export function abbruch(text) {
  console.error(`\n✗ ${text}\n`);
  process.exit(1);
}

/** Befehl ausführen, Ausgabe läuft sichtbar mit; Fehler → Abbruch. */
export function schritt(name, befehl, argumente, umgebung = process.env) {
  console.log(`\n▶ ${name} …\n`);
  const r = spawnSync(befehl, argumente, { stdio: "inherit", env: umgebung });
  if (r.status !== 0) abbruch(`${name} fehlgeschlagen.`);
}

/** Ausgabe eines Befehls lesen (stdout). */
export const lesen = (befehl, argumente) => execFileSync(befehl, argumente, { encoding: "utf8" });

/** Ausgabe von stdout und stderr zusammen, ohne Abbruch; mit Exit-Code. */
export function abfragen(befehl, argumente) {
  const r = spawnSync(befehl, argumente, { encoding: "utf8" });
  return { code: r.status ?? 1, text: `${r.stdout ?? ""}${r.stderr ?? ""}` };
}
