import { describe, expect, it } from "vitest";

import { system } from "./system";
import { aufrufeVon, rustAttrappe } from "./test/hilfen";

describe("system", () => {
  it("ordnerWaehlen liefert den gewählten Pfad", async () => {
    const a = rustAttrappe({ "plugin:dialog|open": () => "/Users/kim/Server" });
    await expect(system.ordnerWaehlen("/Users/kim")).resolves.toBe("/Users/kim/Server");
    const opt = (aufrufeVon(a, "plugin:dialog|open")[0].args as { options: Record<string, unknown> }).options;
    expect(opt).toMatchObject({ directory: true, multiple: false, defaultPath: "/Users/kim" });
  });

  it("ordnerWaehlen: Abbruch und leerer Startpfad", async () => {
    const a = rustAttrappe({ "plugin:dialog|open": () => null });
    await expect(system.ordnerWaehlen("")).resolves.toBeNull();
    const opt = (a[0].args as { options: Record<string, unknown> }).options;
    expect(opt.defaultPath).toBeUndefined();

    rustAttrappe({ "plugin:dialog|open": () => ["/a", "/b"] });
    await expect(system.ordnerWaehlen()).resolves.toBeNull();
  });

  it("oeffnen und version", async () => {
    const a = rustAttrappe({ "plugin:opener|open_url": () => null, "plugin:app|version": () => "0.1.0" });
    await system.oeffnen("https://stellwerk.sieber-bw.de");
    expect((a[0].args as { url: string }).url).toBe("https://stellwerk.sieber-bw.de");
    await expect(system.version()).resolves.toBe("0.1.0");
  });
});
