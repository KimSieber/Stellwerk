import { describe, expect, it } from "vitest";

import { bauUmgebung, gitHinweis, plattformFehler, signaturAuswerten, signaturWaehlen } from "./app-logik.mjs";

const DEV_ID = "Developer ID Application: Kim Jan Sieber (62QD57DH52)";

describe("Signatur wählen", () => {
  it("nimmt das Zertifikat „Developer ID Application“, nicht „Apple Development“", () => {
    const ausgabe = `  1) 9F73 "Apple Development: kim@example.de (7TTFYF5286)"
  2) 0369 "Apple Development: Kim Jan Sieber (7TTFYF5286)"
  3) 7022 "${DEV_ID}"
     3 valid identities found`;
    expect(signaturWaehlen(ausgabe)).toBe(DEV_ID);
  });

  it("ohne Developer ID: null", () => {
    expect(signaturWaehlen(`  1) 9F73 "Apple Development: Kim (7TTFYF5286)"\n     1 valid identities found`)).toBeNull();
    expect(signaturWaehlen("     0 valid identities found")).toBeNull();
  });
});

describe("Plattform", () => {
  it("nur Mac mit Apple Silicon", () => {
    expect(plattformFehler("darwin", "arm64")).toBeNull();
    expect(plattformFehler("darwin", "x64")).toBe("Gebaut wird nur für Apple Silicon (arm64).");
    expect(plattformFehler("linux", "arm64")).toBe("Die App lässt sich nur auf einem Mac bauen.");
  });
});

describe("Git-Hinweis", () => {
  it("still bei sauberem Stand, sonst Anzahl der Dateien", () => {
    expect(gitHinweis("")).toBeNull();
    expect(gitHinweis("\n")).toBeNull();
    expect(gitHinweis(" M src/api.ts\n")).toBe("Achtung: 1 Datei nicht committet – die installierte App entspricht keinem Commit.");
    expect(gitHinweis(" M a\n?? b\n")).toMatch(/^Achtung: 2 Dateien nicht committet/);
  });
});

describe("Bau-Umgebung", () => {
  it("setzt das Zertifikat und lässt Zugangsdaten für die Beglaubigung weg", () => {
    const vorher = { PATH: "/bin", APPLE_ID: "x", APPLE_PASSWORD: "y", APPLE_API_KEY: "k", APPLE_API_ISSUER: "i", APPLE_API_KEY_PATH: "p" };
    const u = bauUmgebung(vorher, DEV_ID);
    expect(u).toEqual({ PATH: "/bin", APPLE_SIGNING_IDENTITY: DEV_ID });
    expect(vorher.APPLE_ID).toBe("x"); // Original bleibt unverändert
  });
});

describe("Signatur der fertigen App", () => {
  const codesign = (autoritaet) => `Executable=/x/Stellwerk.app/Contents/MacOS/stellwerk
Identifier=de.sieber-bw.stellwerk
${autoritaet ? `Authority=${autoritaet}\nAuthority=Developer ID Certification Authority\n` : ""}TeamIdentifier=62QD57DH52
`;

  it("passt: mit Team-Kennung", () => {
    expect(signaturAuswerten(codesign(DEV_ID), DEV_ID)).toEqual({ ok: true, text: `Signiert: ${DEV_ID} · Team 62QD57DH52` });
  });

  it("falsches oder kein Zertifikat", () => {
    expect(signaturAuswerten(codesign("Apple Development: Kim"), DEV_ID)).toEqual({
      ok: false,
      text: `Unerwartete Signatur: Apple Development: Kim (erwartet: ${DEV_ID}).`,
    });
    expect(signaturAuswerten(codesign(null), DEV_ID).text).toBe(`Unerwartete Signatur: keine (erwartet: ${DEV_ID}).`);
  });
});
