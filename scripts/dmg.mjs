// `npm run dmg` (v0.8.4b): signiertes und von Apple beglaubigtes DMG für eine GitHub-Release.
// Installiert nichts – dafür ist `npm run app` da. Bricht ab bei nicht committeten Änderungen
// (eine Release muss einem Commit entsprechen), ohne Developer ID oder ohne hinterlegte
// Zugangsdaten für die Beglaubigung. Die Entscheidungen stecken in app-logik.mjs.

import { existsSync, mkdirSync, readFileSync, statSync, symlinkSync } from "node:fs";

import { abbruch, abfragen, lesen, schritt } from "./ablauf.mjs";
import {
  bauUmgebung,
  beglaubigbarFehler,
  beglaubigungAuswerten,
  beglaubigungFehlerText,
  DMG_INHALT,
  DMG_ORDNER,
  dmgName,
  GEBAUT,
  gitHinweis,
  megabyte,
  NOTAR_PROFIL,
  plattformFehler,
  signaturAuswerten,
  signaturWaehlen,
} from "./app-logik.mjs";

const fehler = plattformFehler(process.platform, process.arch);
if (fehler) abbruch(fehler);

const version = JSON.parse(readFileSync("package.json", "utf8")).version;
const dmg = `${DMG_ORDNER}/${dmgName(version)}`;
console.log(`Stellwerk ${version} – beglaubigtes DMG für eine GitHub-Release`);

const hinweis = gitHinweis(lesen("git", ["status", "--porcelain"]));
if (hinweis) abbruch(`${hinweis}\nFür eine Release bitte zuerst committen.`);

const signatur = signaturWaehlen(lesen("security", ["find-identity", "-v", "-p", "codesigning"]));
if (!signatur) abbruch("Kein Zertifikat „Developer ID Application“ im Schlüsselbund gefunden.");
console.log(`\nSignatur: ${signatur}`);

// Zugangsdaten vorab prüfen – sonst merkt man es erst nach dem Bau
console.log("\n▶ Zugangsdaten für die Beglaubigung prüfen …");
if (abfragen("xcrun", ["notarytool", "history", "--keychain-profile", NOTAR_PROFIL]).code !== 0) {
  abbruch(
    `Keine Zugangsdaten „${NOTAR_PROFIL}“ im Schlüsselbund (oder Apple nicht erreichbar).\n` +
      `Einmalig hinterlegen mit:\n  xcrun notarytool store-credentials ${NOTAR_PROFIL}`,
  );
}

// 1. App bauen und signieren
schritt("App bauen und signieren", "npx", ["tauri", "build", "--bundles", "app"], bauUmgebung(process.env, signatur));
if (!existsSync(GEBAUT)) abbruch(`Gebaute App nicht gefunden: ${GEBAUT}`);
schritt("Signatur prüfen", "codesign", ["--verify", "--deep", "--strict", GEBAUT]);
const angaben = abfragen("codesign", ["-dv", "--verbose=2", GEBAUT]).text;
const pruefung = signaturAuswerten(angaben, signatur);
if (!pruefung.ok) abbruch(pruefung.text);
const nichtBeglaubigbar = beglaubigbarFehler(angaben);
if (nichtBeglaubigbar) abbruch(nichtBeglaubigbar);
console.log(pruefung.text);

// 2. DMG: App plus Verknüpfung zu /Programme zum Hineinziehen
schritt("Inhalt des DMG vorbereiten", "rm", ["-rf", DMG_INHALT]);
mkdirSync(DMG_INHALT, { recursive: true });
mkdirSync(DMG_ORDNER, { recursive: true });
schritt("App kopieren", "ditto", [GEBAUT, `${DMG_INHALT}/Stellwerk.app`]);
symlinkSync("/Applications", `${DMG_INHALT}/Programme`);
schritt("DMG erstellen", "hdiutil", ["create", "-volname", "Stellwerk", "-srcfolder", DMG_INHALT, "-ov", "-format", "UDZO", dmg]);
schritt("DMG signieren", "codesign", ["--sign", signatur, "--timestamp", dmg]);

// 3. Beglaubigung bei Apple (wartet auf das Ergebnis, meist wenige Minuten)
console.log("\n▶ Bei Apple zur Beglaubigung einreichen – das dauert meist 1 bis 5 Minuten …\n");
const antwort = abfragen("xcrun", ["notarytool", "submit", dmg, "--keychain-profile", NOTAR_PROFIL, "--wait"]);
console.log(antwort.text);
const ergebnis = beglaubigungAuswerten(antwort.text);
if (!ergebnis.ok) abbruch(beglaubigungFehlerText(ergebnis));

// 4. Bestätigung ins DMG einheften und prüfen, wie Gatekeeper es sieht
schritt("Bestätigung einheften", "xcrun", ["stapler", "staple", dmg]);
schritt("Gatekeeper-Prüfung", "spctl", ["--assess", "--type", "open", "--context", "context:primary-signature", "-v", dmg]);

console.log(`\n✓ ${dmg} (${megabyte(statSync(dmg).size)})`);
console.log("  Bereit zum Hochladen: GitHub → Releases → „Create a new release“ → Datei hineinziehen.\n");
