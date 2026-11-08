// Auswertung der Ausgaben von `npm run pruefen` (Build, Vitest, cargo llvm-cov)
// zu einer Zusammenfassung auf einen Blick. Reine Funktionen – getestet in
// auswertung.test.mjs. Das Starten der Befehle übernimmt pruefen.mjs.

/** Farb- und Steuerzeichen (ANSI) entfernen – auch die Zeichensatz-Umschaltung, die die
 *  Rust-Testprogramme mit `--color always` hinter jedes „ok“ setzen: unter Linux `\x0f`,
 *  auf dem Mac `ESC ( B`. */
export function ohneFarben(text) {
  // eslint-disable-next-line no-control-regex
  return text.replace(/\x1b\[[0-9;?]*[A-Za-z]/g, "").replace(/\x1b[()][A-Za-z0-9]/g, "").replace(/[\x0e\x0f]/g, "");
}

/**
 * Vitest mit Abdeckung: Anzahl Tests, fehlgeschlagene Tests, Abdeckung (Prozent
 * für Anweisungen, Zweige, Funktionen, Zeilen) und ob eine Schwelle verfehlt wurde.
 */
export function vitestAuswerten(roh) {
  const text = ohneFarben(roh);
  const zeile = text.match(/^\s*Tests\s+(.+)$/m);
  const zahl = (wort) => Number(zeile?.[1].match(new RegExp(`(\\d+) ${wort}`))?.[1] ?? 0);
  const gesamt = Number(zeile?.[1].match(/\((\d+)\)/)?.[1] ?? 0);
  const fehlgeschlagen = [...new Set([...text.matchAll(/^\s*FAIL\s+(.+)$/gm)].map((m) => m[1].trim()))];
  const alle = text.match(/^\s*All files\s*\|\s*([\d.]+)\s*\|\s*([\d.]+)\s*\|\s*([\d.]+)\s*\|\s*([\d.]+)/m);
  const abdeckung = alle
    ? { anweisungen: Number(alle[1]), zweige: Number(alle[2]), funktionen: Number(alle[3]), zeilen: Number(alle[4]) }
    : null;
  const schwelle = [...text.matchAll(/^ERROR: (Coverage for .+)$/gm)].map((m) => m[1].trim());
  return { bestanden: zahl("passed"), fehler: zahl("failed"), gesamt, fehlgeschlagen, abdeckung, schwelle };
}

/** cargo llvm-cov: Summe über alle Testprogramme, fehlgeschlagene Tests, Abdeckung. */
export function rustAuswerten(roh) {
  const text = ohneFarben(roh);
  let bestanden = 0;
  let fehler = 0;
  for (const m of text.matchAll(/test result: \w+\. (\d+) passed; (\d+) failed/g)) {
    bestanden += Number(m[1]);
    fehler += Number(m[2]);
  }
  const fehlgeschlagen = [...new Set([...text.matchAll(/^test (\S+) \.\.\. FAILED$/gm)].map((m) => m[1]))];
  // TOTAL  Regionen Fehlend Abd.%  Funktionen Fehlend Ausgeführt%  Zeilen Fehlend Abd.% …
  const total = text.match(/^TOTAL\s+(.+)$/m)?.[1].trim().split(/\s+/);
  const prozent = (s) => Number(s.replace("%", ""));
  const abdeckung = total ? { funktionen: prozent(total[5]), zeilen: prozent(total[8]) } : null;
  const kompilierfehler = /^error(\[E\d+\])?: /m.test(text) && bestanden + fehler === 0;
  return { bestanden, fehler, fehlgeschlagen, abdeckung, kompilierfehler };
}

/** TypeScript-Fehler aus dem Build (`datei(z,s): error TS…`). */
export function buildAuswerten(roh) {
  return [...ohneFarben(roh).matchAll(/^(.+: error TS\d+: .+)$/gm)].map((m) => m[1].trim());
}

const GRUEN = "\x1b[32m";
const ROT = "\x1b[31m";
const FETT = "\x1b[1m";
const AUS = "\x1b[0m";

const prozente = (a) => Object.values(a).every((p) => p === 100);

/**
 * Zusammenfassung als Text. Je Teil Exit-Code und Ausgabe; `logs`: Pfad der
 * Protokolldatei je Teil; `farbe`: grün/rot für das Terminal.
 */
export function zusammenfassen({ build, frontend, rust }, logs, farbe = false) {
  const zeilen = [];
  const fehlerListe = [];
  const f = (ok, text) => (farbe ? `${ok ? GRUEN : ROT}${text}${AUS}` : text);
  const teil = (ok, name, text) => zeilen.push(f(ok, `${name.padEnd(16)} ${ok ? "✓" : "✗"}  ${text}`.trimEnd()));

  const tsFehler = buildAuswerten(build.ausgabe);
  const buildOk = build.code === 0;
  teil(buildOk, "Build", buildOk ? "" : `${tsFehler.length || "?"} Fehler`);
  if (!buildOk) fehlerListe.push(["Build", tsFehler, logs.build]);

  const v = vitestAuswerten(frontend.ausgabe);
  const testsOk = v.gesamt > 0 && v.fehler === 0;
  teil(testsOk, "Frontend-Tests", v.gesamt > 0 ? `${v.bestanden} von ${v.gesamt} bestanden` : "keine Ergebnisse gefunden");
  const vAbdOk = frontend.code === 0 && v.abdeckung !== null && prozente(v.abdeckung) && v.schwelle.length === 0;
  teil(
    vAbdOk,
    "Frontend-Abd.",
    v.abdeckung
      ? `Zeilen ${v.abdeckung.zeilen} % · Funktionen ${v.abdeckung.funktionen} % · Zweige ${v.abdeckung.zweige} %`
      : "nicht ermittelt",
  );
  if (!testsOk || !vAbdOk) fehlerListe.push(["Frontend", [...v.fehlgeschlagen, ...v.schwelle], logs.frontend]);

  const r = rustAuswerten(rust.ausgabe);
  const rGesamt = r.bestanden + r.fehler;
  const rTestsOk = rGesamt > 0 && r.fehler === 0;
  teil(
    rTestsOk,
    "Rust-Tests",
    r.kompilierfehler ? "Kompilierfehler" : rGesamt > 0 ? `${r.bestanden} von ${rGesamt} bestanden` : "keine Ergebnisse gefunden",
  );
  const rAbdOk = rust.code === 0 && r.abdeckung !== null && prozente(r.abdeckung);
  teil(rAbdOk, "Rust-Abdeckung", r.abdeckung ? `Funktionen ${r.abdeckung.funktionen} % · Zeilen ${r.abdeckung.zeilen} %` : "nicht ermittelt");
  if (!rTestsOk || !rAbdOk) fehlerListe.push(["Rust", r.fehlgeschlagen, logs.rust]);

  const gruen = fehlerListe.length === 0;
  const strich = "═".repeat(48);
  const gesamt = `GESAMT: ${gruen ? "✓ GRÜN" : "✗ ROT"}`;
  const aus = [strich, "Stellwerk – Prüfung", strich, ...zeilen, strich, farbe ? `${FETT}${f(gruen, gesamt)}` : gesamt];
  for (const [name, liste, log] of fehlerListe) {
    aus.push("", `${name}:`);
    for (const t of liste) aus.push(f(false, `  ✗ ${t}`));
    aus.push(`  Einzelheiten: ${log}`);
  }
  return { text: aus.join("\n"), gruen };
}
