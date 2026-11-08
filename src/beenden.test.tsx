import { describe, expect, it } from "vitest";
import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { emit } from "@tauri-apps/api/event";

import { EREIGNIS_BEENDEN } from "./api";
import { BeendenNachfrage, laufOffen, laufOffenMelden, useLaufOffen } from "./beenden";
import { rustAttrappe } from "./test/hilfen";
import { fensterZu, schliessenAnfordern } from "./test/fenster";

/** Wartet, bis die Ereignis-Empfänger angemeldet sind. */
const angemeldet = () => act(() => new Promise((r) => setTimeout(r, 0)));

describe("Beenden – Nachfrage bei offenem Lauf (v0.6.1)", () => {
  it("ohne offenen Lauf: Fenster schließt, ⌘Q beendet sofort", async () => {
    rustAttrappe({});
    render(<BeendenNachfrage />);
    await angemeldet();
    expect(schliessenAnfordern()).toBe(false);
    await act(() => emit(EREIGNIS_BEENDEN));
    expect(fensterZu).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  it("mit offenem Lauf: Nachfrage – weiterarbeiten oder trotzdem beenden", async () => {
    rustAttrappe({});
    const ab = laufOffenMelden();
    render(<BeendenNachfrage />);
    await angemeldet();
    let verhindert = false;
    act(() => {
      verhindert = schliessenAnfordern();
    });
    expect(verhindert).toBe(true);
    expect(screen.getByRole("alertdialog", { name: "Stellwerk beenden" })).toHaveTextContent("gilt der Lauf als unterbrochen");
    await userEvent.click(screen.getByRole("button", { name: "Weiterarbeiten" }));
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(fensterZu).not.toHaveBeenCalled();

    await act(() => emit(EREIGNIS_BEENDEN));
    await userEvent.click(screen.getByRole("button", { name: "Trotzdem beenden" }));
    expect(fensterZu).toHaveBeenCalledTimes(1);
    ab();
    expect(laufOffen()).toBe(false);
  });

  it("useLaufOffen meldet an und beim Ende wieder ab", () => {
    function Lauf({ aktiv }: { aktiv: boolean }) {
      useLaufOffen(aktiv);
      return null;
    }
    const r = render(<Lauf aktiv={false} />);
    expect(laufOffen()).toBe(false);
    r.rerender(<Lauf aktiv />);
    expect(laufOffen()).toBe(true);
    r.unmount();
    expect(laufOffen()).toBe(false);
  });
});
