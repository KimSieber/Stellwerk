// `npm run pruefen`: Build, Frontend-Tests mit Abdeckung und Rust-Tests mit
// Abdeckung nacheinander. Die Ausgabe läuft wie gewohnt (farbig) mit und wird
// zusätzlich in target/pruefen/*.log gesichert; am Ende steht eine
// Zusammenfassung auf einen Blick. Alle drei Teile laufen
// auch dann, wenn ein früherer rot ist – so ist die Zusammenfassung vollständig.
// Exit-Code 1, wenn etwas rot ist.

import { spawn } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";

import { zusammenfassen } from "./auswertung.mjs";

const ORDNER = "target/pruefen";

const TEILE = [
  ["build", "Build", "npm run build"],
  ["frontend", "Frontend-Tests mit Abdeckung", "npm run abdeckung"],
  ["rust", "Rust-Tests mit Abdeckung", "npm run abdeckung:rust"],
];

function ausfuehren(befehl) {
  return new Promise((fertig) => {
    let ausgabe = "";
    // Farben behalten, obwohl die Ausgabe durch eine Pipe läuft
    const p = spawn(befehl, { shell: true, env: { ...process.env, FORCE_COLOR: "1", CARGO_TERM_COLOR: "always" } });
    const mit = (strom) => (d) => {
      ausgabe += d;
      strom.write(d);
    };
    p.stdout.on("data", mit(process.stdout));
    p.stderr.on("data", mit(process.stderr));
    p.on("close", (code) => fertig({ code: code ?? 1, ausgabe }));
  });
}

mkdirSync(ORDNER, { recursive: true });
const ergebnisse = {};
const logs = {};
for (const [schluessel, name, befehl] of TEILE) {
  const start = Date.now();
  console.log(`\n▶ ${name} …\n`);
  ergebnisse[schluessel] = await ausfuehren(befehl);
  logs[schluessel] = `${ORDNER}/${schluessel}.log`;
  writeFileSync(logs[schluessel], ergebnisse[schluessel].ausgabe);
  const ok = ergebnisse[schluessel].code === 0 ? "✓" : "✗";
  console.log(`\n▶ ${name} ${ok} (${Math.round((Date.now() - start) / 1000)} s)`);
}

const { text, gruen } = zusammenfassen(ergebnisse, logs, process.stdout.isTTY);
console.log(`\n${text}\n`);
process.exit(gruen ? 0 : 1);
