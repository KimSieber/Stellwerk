import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { Zustand } from "../test/hilfen";
import type { ObereRegel } from "./modell";
import { StandardRegeln } from "./StandardRegeln";

const regeln: ObereRegel[] = [
  { muster: ".DS_Store", typ: "ignorieren", herkunft: "global" },
  { muster: "Icon[\r]", typ: "ignorieren", herkunft: "global" },
  { muster: "logs/", typ: "schuetzen", herkunft: "strecke" },
];

describe("StandardRegeln", () => {
  it("zeigt Regeln mit Typ, Herkunft und Zählung", () => {
    render(<StandardRegeln titel="Standardregeln" regeln={regeln} aus={["logs/"]} onChange={() => {}} />);
    expect(screen.getByText("Standardregeln – 2 von 3 wirksam")).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: ".DS_Store" })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: "logs/" })).not.toBeChecked();
    // Steuerzeichen lesbar dargestellt
    expect(screen.getByText("Icon[\\r]")).toBeInTheDocument();
    expect(screen.getByText("Schützen")).toBeInTheDocument();
    expect(screen.getByText("Strecke")).toBeInTheDocument();
    expect(screen.getAllByText("global")).toHaveLength(2);
  });

  it("ab- und wieder anschalten", async () => {
    const f = vi.fn();
    render(
      <Zustand<string[]>
        start={[]}
        beobachter={f}
        kind={(aus, setze) => <StandardRegeln titel="T" regeln={regeln} aus={aus} onChange={setze} />}
      />,
    );
    await userEvent.click(screen.getByRole("checkbox", { name: ".DS_Store" }));
    expect(f).toHaveBeenLastCalledWith([".DS_Store"]);
    await userEvent.click(screen.getByRole("checkbox", { name: "logs/" }));
    expect(f).toHaveBeenLastCalledWith([".DS_Store", "logs/"]);
    await userEvent.click(screen.getByRole("checkbox", { name: ".DS_Store" }));
    expect(f).toHaveBeenLastCalledWith(["logs/"]);
  });

  it("ohne übergeordnete Regeln", () => {
    render(<StandardRegeln titel="T" regeln={[]} aus={[]} onChange={() => {}} />);
    expect(screen.getByText("Keine übergeordneten Regeln.")).toBeInTheDocument();
    expect(screen.getByText("T – 0 von 0 wirksam")).toBeInTheDocument();
  });
});
