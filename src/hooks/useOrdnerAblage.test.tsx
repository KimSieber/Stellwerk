import { afterEach, describe, expect, it, vi } from "vitest";
import { act, render, screen } from "@testing-library/react";

import { ablageEmpfaenger, ablageSenden } from "../test/ablage";
import { useOrdnerAblage } from "./useOrdnerAblage";

/** Zwei Zielfelder; elementFromPoint liefert „a“ für y < 100, „b“ ab 100 (in Seitenkoordinaten). */
function Pruefling({ onAblage }: { onAblage: (z: string, p: string, m: boolean) => void }) {
  const { aktiv, wartet } = useOrdnerAblage(onAblage);
  return (
    <>
      <div data-drop-target="a">
        <span data-testid="in-a">A</span>
      </div>
      <div data-drop-target="b" />
      <output data-testid="zustand">{`${aktiv ?? "-"}|${wartet}`}</output>
    </>
  );
}

/** Seitenaufbau: Feld „a“ bei y 0–49, nichts bei 50–99, Feld „b“ ab 100. */
function seite() {
  const a = screen.getByTestId("in-a");
  const b = document.querySelector('[data-drop-target="b"]')!;
  document.elementFromPoint = vi.fn((_x: number, y: number) =>
    y >= 0 && y < 50 ? a : y >= 100 ? b : null,
  ) as typeof document.elementFromPoint;
}

const zustand = () => screen.getByTestId("zustand").textContent;
const pos = (x: number, y: number) => ({ x, y }) as never;

afterEach(() => {
  // @ts-expect-error – jsdom kennt elementFromPoint nicht; im Test gesetzt
  delete document.elementFromPoint;
});

describe("useOrdnerAblage", () => {
  it("Ziehen, Hervorheben, Ablegen auf ein Zielfeld", async () => {
    const ab = vi.fn();
    render(<Pruefling onAblage={ab} />);
    await act(async () => {}); // Anmeldung abwarten
    seite();

    act(() => ablageSenden({ type: "enter", paths: ["/x"], position: pos(5, 10) }));
    expect(zustand()).toBe("-|true");
    act(() => ablageSenden({ type: "over", position: pos(5, 10) }));
    expect(zustand()).toBe("a|true");
    act(() => ablageSenden({ type: "over", position: pos(5, 150) }));
    expect(zustand()).toBe("b|true");
    act(() => ablageSenden({ type: "drop", paths: ["/Users/kim/Server"], position: pos(5, 150) }));
    expect(ab).toHaveBeenCalledWith("b", "/Users/kim/Server", false);
    expect(zustand()).toBe("-|false");
  });

  it("ermittelt den Versatz der Titelleiste je Ziehvorgang", async () => {
    const ab = vi.fn();
    render(<Pruefling onAblage={ab} />);
    await act(async () => {});
    // Bei y=80 trifft erst ein Versatz von 31 (80-31 = 49) das Feld a
    document.elementFromPoint = vi.fn((_x: number, y: number) =>
      y < 50 ? screen.getByTestId("in-a") : null,
    ) as typeof document.elementFromPoint;
    act(() => ablageSenden({ type: "enter", paths: ["/x"], position: pos(5, 80) }));
    act(() => ablageSenden({ type: "drop", paths: ["/a", "/b"], position: pos(5, 80) }));
    expect(ab).toHaveBeenCalledWith("a", "/a", true);
    const aufrufe = vi.mocked(document.elementFromPoint).mock.calls.map((c) => c[1]);
    expect(aufrufe).toContain(49);
  });

  it("außerhalb der Zielfelder, ohne Pfad oder abgebrochen: nichts übernehmen", async () => {
    const ab = vi.fn();
    render(<Pruefling onAblage={ab} />);
    await act(async () => {});
    document.elementFromPoint = vi.fn(() => null) as typeof document.elementFromPoint;

    act(() => ablageSenden({ type: "enter", paths: ["/x"], position: pos(5, 10) }));
    act(() => ablageSenden({ type: "over", position: pos(5, 10) }));
    expect(zustand()).toBe("-|true");
    act(() => ablageSenden({ type: "drop", paths: ["/x"], position: pos(5, 10) }));

    seite();
    act(() => ablageSenden({ type: "drop", paths: [], position: pos(5, 10) }));
    // Versatz ermittelt (über Feld a), abgelegt aber in der Lücke zwischen den Feldern
    act(() => ablageSenden({ type: "enter", paths: ["/x"], position: pos(5, 10) }));
    act(() => ablageSenden({ type: "over", position: pos(5, 10) }));
    act(() => ablageSenden({ type: "drop", paths: ["/x"], position: pos(5, 70) }));
    expect(ab).not.toHaveBeenCalled();

    act(() => ablageSenden({ type: "enter", paths: ["/x"], position: pos(5, 10) }));
    act(() => ablageSenden({ type: "over", position: pos(5, 10) }));
    act(() => ablageSenden({ type: "leave" }));
    expect(zustand()).toBe("-|false");
  });

  it("verhindert, dass das Fenster die Datei selbst öffnet, und meldet sich ab", async () => {
    const { unmount } = render(<Pruefling onAblage={vi.fn()} />);
    await act(async () => {});
    const e = new Event("drop", { cancelable: true });
    window.dispatchEvent(e);
    expect(e.defaultPrevented).toBe(true);
    expect(ablageEmpfaenger).toHaveLength(1);
    unmount();
    await act(async () => {});
    expect(ablageEmpfaenger).toHaveLength(0);
    const nachher = new Event("dragover", { cancelable: true });
    window.dispatchEvent(nachher);
    expect(nachher.defaultPrevented).toBe(false);
  });
});
