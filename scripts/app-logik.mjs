// Logik für `npm run app` (v0.8.4): App bauen, signieren und in /Programme ersetzen.
// Reine Funktionen – getestet in app-logik.test.mjs. Das Starten der Befehle übernimmt app.mjs.

/** Ziel der installierten App. */
export const PROGRAMM = "/Applications/Stellwerk.app";
/** Ergebnis von `tauri build --bundles app` (Cargo-Workspace: target/ im Projektordner). */
export const GEBAUT = "target/release/bundle/macos/Stellwerk.app";

/**
 * Signatur-Zertifikat aus der Ausgabe von `security find-identity -v -p codesigning`:
 * das erste „Developer ID Application“ (Verteilung außerhalb des App Stores).
 * „Apple Development“ taugt dafür nicht. Ohne Treffer: null.
 */
export function signaturWaehlen(ausgabe) {
  return ausgabe.match(/"(Developer ID Application: [^"]+)"/)?.[1] ?? null;
}

/** Nur auf einem Mac mit Apple Silicon (Vorgabe v0.7: nur Apple Silicon). */
export function plattformFehler(plattform, architektur) {
  if (plattform !== "darwin") return "Die App lässt sich nur auf einem Mac bauen.";
  if (architektur !== "arm64") return "Gebaut wird nur für Apple Silicon (arm64).";
  return null;
}

/** Hinweis bei nicht committeten Änderungen (`git status --porcelain`); sonst null. */
export function gitHinweis(porcelain) {
  const dateien = porcelain.split("\n").filter((z) => z.trim() !== "");
  if (dateien.length === 0) return null;
  const n = dateien.length === 1 ? "1 Datei" : `${dateien.length} Dateien`;
  return `Achtung: ${n} nicht committet – die installierte App entspricht keinem Commit.`;
}

/** Umgebung für den Bau: Tauri signiert mit diesem Zertifikat; ohne Zugangsdaten keine Beglaubigung. */
export function bauUmgebung(umgebung, signatur) {
  const neu = { ...umgebung, APPLE_SIGNING_IDENTITY: signatur };
  // Beglaubigung erst mit `npm run dmg` (v0.8.4b) – hier bewusst nicht
  for (const k of ["APPLE_ID", "APPLE_PASSWORD", "APPLE_API_KEY", "APPLE_API_ISSUER", "APPLE_API_KEY_PATH"]) delete neu[k];
  return neu;
}

/**
 * Prüfung der fertigen App aus `codesign -dv --verbose=2` (Ausgabe auf stderr):
 * signiert mit dem erwarteten Zertifikat? Liefert Team-Kennung oder eine Fehlermeldung.
 */
export function signaturAuswerten(ausgabe, signatur) {
  const autoritaet = ausgabe.match(/^Authority=(.+)$/m)?.[1];
  const team = ausgabe.match(/^TeamIdentifier=(.+)$/m)?.[1];
  if (autoritaet !== signatur) {
    return { ok: false, text: `Unerwartete Signatur: ${autoritaet ?? "keine"} (erwartet: ${signatur}).` };
  }
  return { ok: true, text: `Signiert: ${autoritaet} · Team ${team}` };
}
