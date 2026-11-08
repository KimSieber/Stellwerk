import { describe, expect, it } from "vitest";

import { buildAuswerten, ohneFarben, rustAuswerten, vitestAuswerten, zusammenfassen } from "./auswertung.mjs";

// Ausschnitte im Format der echten Ausgaben
const VITEST_GRUEN = `
 ✓ src/api.test.ts (20 tests) 31ms
 \x1b[32m✓\x1b[39m src/App.test.tsx (12 tests) 900ms

 Test Files  31 passed (31)
      Tests  259 passed (259)
-------------------|---------|----------|---------|---------|-------------------
File               | % Stmts | % Branch | % Funcs | % Lines | Uncovered Line #s
-------------------|---------|----------|---------|---------|-------------------
All files          |     100 |      100 |     100 |     100 |
`;

const VITEST_ROT = `
 FAIL  src/strecke/StreckeAnsicht.test.tsx > Streckenansicht > zählt richtig
 FAIL  src/strecke/StreckeAnsicht.test.tsx > Streckenansicht > zählt richtig
 FAIL  src/api.test.ts > api > ruft auf
      Tests  2 failed | 257 passed (259)
All files          |     100 |    99.83 |     100 |     100 |
ERROR: Coverage for branches (99.83%) does not meet global threshold (100%)
`;

const RUST_GRUEN = `
running 3 tests
test a ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 47 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.79s
Filename                      Regions    Missed Regions     Cover   Functions  Missed Functions  Executed       Lines      Missed Lines     Cover    Branches   Missed Branches     Cover
TOTAL                            7368               120    98.37%         591                 0   100.00%        3568                 0   100.00%           0                 0         -
`;

const RUST_ROT = `
test ftps_erst_mit_dem_server ... FAILED
test vergleich_fehler ... FAILED
test result: FAILED. 45 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.14s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.1s
`;

const RUST_KOMPILIERT_NICHT = `
error[E0425]: cannot find value \`x\` in this scope
error: could not compile \`stellwerk\` (lib) due to 1 previous error
`;

const LOGS = { build: "target/pruefen/build.log", frontend: "target/pruefen/frontend.log", rust: "target/pruefen/rust.log" };
const ok = (ausgabe) => ({ code: 0, ausgabe });
const rot = (ausgabe) => ({ code: 1, ausgabe });

describe("Auswertung der Prüfung", () => {
  it("entfernt Farbzeichen", () => {
    expect(ohneFarben("\x1b[32m✓\x1b[39m grün\x1b[2K")).toBe("✓ grün");
  });

  it("Vitest grün und rot", () => {
    expect(vitestAuswerten(VITEST_GRUEN)).toEqual({
      bestanden: 259,
      fehler: 0,
      gesamt: 259,
      fehlgeschlagen: [],
      abdeckung: { anweisungen: 100, zweige: 100, funktionen: 100, zeilen: 100 },
      schwelle: [],
    });
    const r = vitestAuswerten(VITEST_ROT);
    expect(r).toMatchObject({ bestanden: 257, fehler: 2, gesamt: 259 });
    expect(r.fehlgeschlagen).toEqual([
      "src/strecke/StreckeAnsicht.test.tsx > Streckenansicht > zählt richtig",
      "src/api.test.ts > api > ruft auf",
    ]);
    expect(r.abdeckung?.zweige).toBe(99.83);
    expect(r.schwelle).toEqual(["Coverage for branches (99.83%) does not meet global threshold (100%)"]);
    expect(vitestAuswerten("nichts")).toMatchObject({ bestanden: 0, fehler: 0, gesamt: 0, abdeckung: null });
  });

  it("Rust: Summe über alle Testprogramme, Fehlschläge, Abdeckung, Kompilierfehler", () => {
    expect(rustAuswerten(RUST_GRUEN)).toEqual({
      bestanden: 50,
      fehler: 0,
      fehlgeschlagen: [],
      abdeckung: { funktionen: 100, zeilen: 100 },
      kompilierfehler: false,
    });
    expect(rustAuswerten(RUST_ROT)).toEqual({
      bestanden: 55,
      fehler: 2,
      fehlgeschlagen: ["ftps_erst_mit_dem_server", "vergleich_fehler"],
      abdeckung: null,
      kompilierfehler: false,
    });
    expect(rustAuswerten(RUST_KOMPILIERT_NICHT).kompilierfehler).toBe(true);
  });

  it("Build: TypeScript-Fehler", () => {
    expect(buildAuswerten("src/a.ts(3,1): error TS2322: Typ falsch\nsonst nichts")).toEqual(["src/a.ts(3,1): error TS2322: Typ falsch"]);
  });

  it("alles grün: eine Zeile je Teil, GESAMT grün", () => {
    const { text, gruen } = zusammenfassen({ build: ok(""), frontend: ok(VITEST_GRUEN), rust: ok(RUST_GRUEN) }, LOGS);
    expect(gruen).toBe(true);
    expect(text.split("\n")).toEqual([
      "═".repeat(48),
      "Stellwerk – Prüfung",
      "═".repeat(48),
      "Build            ✓",
      "Frontend-Tests   ✓  259 von 259 bestanden",
      "Frontend-Abd.    ✓  Zeilen 100 % · Funktionen 100 % · Zweige 100 %",
      "Rust-Tests       ✓  50 von 50 bestanden",
      "Rust-Abdeckung   ✓  Funktionen 100 % · Zeilen 100 %",
      "═".repeat(48),
      "GESAMT: ✓ GRÜN",
    ]);
  });

  it("alles rot: Fehlschläge mit Namen und Protokolldatei", () => {
    const { text, gruen } = zusammenfassen(
      { build: rot("src/a.ts(3,1): error TS2322: Typ falsch"), frontend: rot(VITEST_ROT), rust: rot(RUST_ROT) },
      LOGS,
    );
    expect(gruen).toBe(false);
    expect(text).toContain("Build            ✗  1 Fehler");
    expect(text).toContain("Frontend-Tests   ✗  257 von 259 bestanden");
    expect(text).toContain("Frontend-Abd.    ✗  Zeilen 100 % · Funktionen 100 % · Zweige 99.83 %");
    expect(text).toContain("Rust-Tests       ✗  55 von 57 bestanden");
    expect(text).toContain("Rust-Abdeckung   ✗  nicht ermittelt");
    expect(text).toContain("GESAMT: ✗ ROT");
    expect(text).toContain("Build:\n  ✗ src/a.ts(3,1): error TS2322: Typ falsch\n  Einzelheiten: target/pruefen/build.log");
    expect(text).toContain("  ✗ src/api.test.ts > api > ruft auf");
    expect(text).toContain("  ✗ Coverage for branches (99.83%) does not meet global threshold (100%)");
    expect(text).toContain("Rust:\n  ✗ ftps_erst_mit_dem_server\n  ✗ vergleich_fehler\n  Einzelheiten: target/pruefen/rust.log");
  });

  it("nichts auswertbar: Kompilierfehler, fehlende Ergebnisse, Abdeckung unter 100 % ohne Testfehler", () => {
    const { text } = zusammenfassen({ build: rot("kaputt"), frontend: rot("nichts"), rust: rot(RUST_KOMPILIERT_NICHT) }, LOGS);
    expect(text).toContain("Build            ✗  ? Fehler");
    expect(text).toContain("Frontend-Tests   ✗  keine Ergebnisse gefunden");
    expect(text).toContain("Frontend-Abd.    ✗  nicht ermittelt");
    expect(text).toContain("Rust-Tests       ✗  Kompilierfehler");

    const leer = zusammenfassen({ build: ok(""), frontend: ok(VITEST_GRUEN), rust: rot("") }, LOGS).text;
    expect(leer).toContain("Rust-Tests       ✗  keine Ergebnisse gefunden");

    const teilweise = RUST_GRUEN.replace(/100\.00%/, "99.50%");
    const r = zusammenfassen({ build: ok(""), frontend: ok(VITEST_GRUEN), rust: rot(teilweise) }, LOGS);
    expect(r.text).toContain("Rust-Tests       ✓  50 von 50 bestanden");
    expect(r.text).toContain("Rust-Abdeckung   ✗  Funktionen 99.5 % · Zeilen 100 %");
    expect(r.gruen).toBe(false);
  });

  it("mit Farbe: grün/rot je Zeile, GESAMT fett, Fehlschläge rot", () => {
    const gruen = zusammenfassen({ build: ok(""), frontend: ok(VITEST_GRUEN), rust: ok(RUST_GRUEN) }, LOGS, true).text;
    expect(gruen).toContain("\x1b[32mBuild            ✓\x1b[0m");
    expect(gruen).toContain("\x1b[1m\x1b[32mGESAMT: ✓ GRÜN\x1b[0m");
    const rotText = zusammenfassen({ build: ok(""), frontend: ok(VITEST_GRUEN), rust: rot(RUST_ROT) }, LOGS, true).text;
    expect(rotText).toContain("\x1b[31mRust-Tests       ✗  55 von 57 bestanden\x1b[0m");
    expect(rotText).toContain("\x1b[31m  ✗ vergleich_fehler\x1b[0m");
    expect(rotText).toContain("\x1b[1m\x1b[31mGESAMT: ✗ ROT\x1b[0m");
    expect(ohneFarben(rotText)).toBe(zusammenfassen({ build: ok(""), frontend: ok(VITEST_GRUEN), rust: rot(RUST_ROT) }, LOGS).text);
  });
});
