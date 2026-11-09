// `npm run app` (v0.8.4): Stellwerk bauen, mit der Developer ID signieren und die App in
// /Programme ersetzen. Nur auf dem Mac (Apple Silicon), ohne GitHub Actions.
// Bricht ab, wenn Stellwerk läuft oder kein Zertifikat „Developer ID Application“ da ist;
// warnt bei nicht committeten Änderungen. Die Entscheidungen stecken in app-logik.mjs.

import { execFileSync, spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";

import { bauUmgebung, GEBAUT, gitHinweis, plattformFehler, PROGRAMM, signaturAuswerten, signaturWaehlen } from "./app-logik.mjs";

function abbruch(text) {
  console.error(`\n✗ ${text}\n`);
  process.exit(1);
}

/** Befehl ausführen, Ausgabe läuft sichtbar mit; Fehler → Abbruch. */
function schritt(name, befehl, argumente, umgebung = process.env) {
  console.log(`\n▶ ${name} …\n`);
  const r = spawnSync(befehl, argumente, { stdio: "inherit", env: umgebung });
  if (r.status !== 0) abbruch(`${name} fehlgeschlagen.`);
}

const lesen = (befehl, argumente) => execFileSync(befehl, argumente, { encoding: "utf8" });

const fehler = plattformFehler(process.platform, process.arch);
if (fehler) abbruch(fehler);

const version = JSON.parse(readFileSync("package.json", "utf8")).version;
console.log(`Stellwerk ${version} – App bauen und in /Programme ersetzen`);

// Läuft die installierte App? Dann nicht überschreiben.
if (spawnSync("pgrep", ["-f", `${PROGRAMM}/Contents/MacOS/`]).status === 0) {
  abbruch("Stellwerk läuft noch – bitte beenden (⌘Q) und erneut starten.");
}

const hinweis = gitHinweis(lesen("git", ["status", "--porcelain"]));
if (hinweis) console.log(`\n⚠ ${hinweis}`);

const signatur = signaturWaehlen(lesen("security", ["find-identity", "-v", "-p", "codesigning"]));
if (!signatur) abbruch("Kein Zertifikat „Developer ID Application“ im Schlüsselbund gefunden.");
console.log(`\nSignatur: ${signatur}`);

schritt("App bauen und signieren", "npx", ["tauri", "build", "--bundles", "app"], bauUmgebung(process.env, signatur));
if (!existsSync(GEBAUT)) abbruch(`Gebaute App nicht gefunden: ${GEBAUT}`);

schritt("Signatur prüfen", "codesign", ["--verify", "--deep", "--strict", GEBAUT]);
const pruefung = signaturAuswerten(spawnSync("codesign", ["-dv", "--verbose=2", GEBAUT], { encoding: "utf8" }).stderr, signatur);
if (!pruefung.ok) abbruch(pruefung.text);
console.log(pruefung.text);

schritt("Alte App entfernen", "rm", ["-rf", PROGRAMM]);
schritt("Neue App nach /Programme kopieren", "ditto", [GEBAUT, PROGRAMM]);

console.log(`\n✓ Stellwerk ${version} liegt in /Programme/Stellwerk.app\n`);
