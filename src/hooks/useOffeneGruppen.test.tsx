import { afterEach, describe, expect, it, vi } from "vitest";
import { act, renderHook } from "@testing-library/react";

import { SPEICHER_SCHLUESSEL, useOffeneGruppen } from "./useOffeneGruppen";

afterEach(() => vi.restoreAllMocks());

describe("aufgeklappte Gruppen", () => {
  it("anfangs alle zu; Umschalten und „alle“ werden gemerkt", () => {
    const { result } = renderHook(() => useOffeneGruppen());
    expect(result.current.istOffen("APIs")).toBe(false);

    act(() => result.current.umschalten("APIs"));
    expect(result.current.istOffen("APIs")).toBe(true);
    expect(JSON.parse(localStorage.getItem(SPEICHER_SCHLUESSEL)!)).toEqual(["APIs"]);

    act(() => result.current.umschalten("APIs"));
    expect(result.current.istOffen("APIs")).toBe(false);

    act(() => result.current.alleSetzen(["APIs", ""]));
    expect([result.current.istOffen("APIs"), result.current.istOffen("")]).toEqual([true, true]);
    act(() => result.current.alleSetzen([]));
    expect(result.current.istOffen("APIs")).toBe(false);
  });

  it("liest den gemerkten Stand beim Start", () => {
    localStorage.setItem(SPEICHER_SCHLUESSEL, JSON.stringify(["Webseiten", 3]));
    const { result } = renderHook(() => useOffeneGruppen());
    expect([result.current.istOffen("Webseiten"), result.current.istOffen("3")]).toEqual([true, false]);
  });

  it("unbrauchbarer Speicherinhalt: alle zu", () => {
    for (const inhalt of ["kein json", JSON.stringify({ a: 1 })]) {
      localStorage.setItem(SPEICHER_SCHLUESSEL, inhalt);
      const { result } = renderHook(() => useOffeneGruppen());
      expect(result.current.istOffen("a")).toBe(false);
    }
  });

  it("Speicher nicht verfügbar: funktioniert trotzdem bis zum Schließen", () => {
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("voll");
    });
    const { result } = renderHook(() => useOffeneGruppen());
    act(() => result.current.umschalten("APIs"));
    expect(result.current.istOffen("APIs")).toBe(true);
  });
});
