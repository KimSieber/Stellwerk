import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import type { Endpunkt, Verbindungstest as Ergebnis } from "../api";
import { aufrufeVon, mitQuery, rustAttrappe, Zustand } from "../test/hilfen";
import { EndpunktFelder } from "./Felder";
import { Verbindungstest } from "./Verbindungstest";

type Ftps = Endpunkt & { typ: "ftps" };
const ziel: Ftps = {
  typ: "ftps", host: "ftp.example.de", port: 21, pfad: "/httpdocs", benutzer: "kim", tls: "explizit", konto_ref: "stellwerk:s:z",
};
const FP = Array(32).fill("AB").join(":");

function ergebnis(teil: Partial<Ergebnis> = {}): Ergebnis {
  return {
    begruessung: "Willkommen",
    verzeichnis: "/httpdocs",
    eintraege: 1,
    faehigkeiten: { mlsd: true, mfmt: true, chmod: true, fortsetzen: true, utf8: true },
    zertifikat: { fingerprint: FP, vom_system: true },
    ...teil,
  };
}

const knopf = () => screen.getByRole("button", { name: /Verbindung testen|Teste Verbindung/ });

describe("Verbindung testen", () => {
  it("zeigt den Lauf und danach Verbindung, Pfad und Fähigkeiten", async () => {
    let fertig: (e: Ergebnis) => void = () => {};
    const a = rustAttrappe({ verbindung_testen: () => new Promise<Ergebnis>((r) => (fertig = r)) });
    render(<Verbindungstest ziel={ziel} passwort="" onZertifikat={() => {}} />);
    await userEvent.click(knopf());
    expect(knopf()).toHaveTextContent("Teste Verbindung …");
    expect(knopf()).toBeDisabled();
    // Leeres Passwort: Rust nimmt das aus dem Schlüsselbund
    expect(aufrufeVon(a, "verbindung_testen")[0].args).toMatchObject({ ziel, passwort: null });

    fertig(ergebnis());
    const ok = await screen.findByRole("status");
    expect(ok).toHaveTextContent("✓ Verbunden mit ftp.example.de");
    expect(ok).toHaveTextContent("Serverpfad /httpdocs · 1 Eintrag");
    expect(ok).toHaveTextContent("„Willkommen“");
    expect(ok).toHaveTextContent("vom System anerkannt");
    expect(ok).toHaveTextContent(/MLSD\)ja/);
    expect(ok).toHaveTextContent(/MFMT\)ja/);
    expect(ok).toHaveTextContent(/CHMOD\)ja/);
    expect(ok).toHaveTextContent(/REST\)ja/);
    expect(knopf()).toHaveTextContent("Verbindung testen");
  });

  it("eingeschränkter Server über bestätigten Fingerprint", async () => {
    const e = ergebnis({
      begruessung: null,
      eintraege: 2,
      faehigkeiten: { mlsd: false, mfmt: false, chmod: null, fortsetzen: false, utf8: false },
      zertifikat: { fingerprint: FP, vom_system: false },
    });
    const a = rustAttrappe({ verbindung_testen: () => e });
    render(<Verbindungstest ziel={ziel} passwort="geheim" onZertifikat={() => {}} />);
    await userEvent.click(knopf());
    const ok = await screen.findByRole("status");
    expect(aufrufeVon(a, "verbindung_testen")[0].args).toMatchObject({ passwort: "geheim" });
    expect(ok).toHaveTextContent("2 Einträge");
    expect(ok).not.toHaveTextContent("„");
    expect(ok).toHaveTextContent("über den bestätigten Fingerprint");
    expect(ok).toHaveTextContent("nein – einfache Liste (LIST)");
    expect(ok).toHaveTextContent("nein – Vergleich über das Manifest");
    expect(ok).toHaveTextContent("unbekannt – wird beim ersten Setzen geprüft");
    expect(ok).toHaveTextContent(/REST\)nein/);
  });

  it("SITE CHMOD ausdrücklich nicht vorhanden", async () => {
    rustAttrappe({ verbindung_testen: () => ergebnis({ faehigkeiten: { ...ergebnis().faehigkeiten, chmod: false } }) });
    render(<Verbindungstest ziel={ziel} passwort="" onZertifikat={() => {}} />);
    await userEvent.click(knopf());
    expect(await screen.findByRole("status")).toHaveTextContent(/CHMOD\)nein/);
  });

  it("nicht anerkanntes Zertifikat: Fingerprint zeigen und bewusst bestätigen", async () => {
    rustAttrappe({
      verbindung_testen: () => {
        throw { kategorie: "zertifikat", meldung: "Zertifikat nicht anerkannt: UnknownIssuer", fingerprint: FP };
      },
    });
    const vertrauen = vi.fn();
    render(<Verbindungstest ziel={ziel} passwort="" onZertifikat={vertrauen} />);
    await userEvent.click(knopf());
    expect(await screen.findByText("✗ Zertifikat nicht anerkannt")).toBeInTheDocument();
    expect(screen.getByText(/UnknownIssuer/)).toBeInTheDocument();
    expect(screen.getByText(FP)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Diesem Zertifikat vertrauen" }));
    expect(vertrauen).toHaveBeenCalledWith(FP);
  });

  it("andere Fehler als Text – auch ohne Fehlerobjekt", async () => {
    let fehler: unknown = { kategorie: "anmeldung", meldung: "Anmeldung: Benutzer oder Passwort abgelehnt" };
    rustAttrappe({
      verbindung_testen: () => {
        throw fehler;
      },
    });
    render(<Verbindungstest ziel={ziel} passwort="" onZertifikat={() => {}} />);
    await userEvent.click(knopf());
    expect(await screen.findByText("✗ Anmeldung: Benutzer oder Passwort abgelehnt")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Diesem Zertifikat vertrauen" })).toBeNull();
    fehler = null;
    await userEvent.click(knopf());
    expect(await screen.findByText(/^✗ /)).not.toHaveTextContent("Anmeldung");
  });

  it("Zertifikat-Fehler ohne Fingerprint bietet kein Vertrauen an", async () => {
    rustAttrappe({
      verbindung_testen: () => {
        throw { kategorie: "zertifikat", meldung: "kaputt" };
      },
    });
    render(<Verbindungstest ziel={ziel} passwort="" onZertifikat={() => {}} />);
    await userEvent.click(knopf());
    expect(await screen.findByText("✗ kaputt")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Diesem Zertifikat vertrauen" })).toBeNull();
  });

  it("bestätigtes Zertifikat anzeigen und entfernen; ohne Server kein Test", async () => {
    const f = vi.fn();
    const { rerender } = render(<Verbindungstest ziel={{ ...ziel, zertifikat: FP }} passwort="" onZertifikat={f} />);
    expect(screen.getByText(FP)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Bestätigung entfernen" }));
    expect(f).toHaveBeenCalledWith(undefined);
    rerender(<Verbindungstest ziel={{ ...ziel, host: " " }} passwort="" onZertifikat={f} />);
    expect(knopf()).toBeDisabled();
    expect(screen.queryByText(/Bestätigtes Zertifikat/)).toBeNull();
  });

  it("Ergebnis verschwindet, sobald sich die Angaben ändern", async () => {
    rustAttrappe({ verbindung_testen: () => ergebnis() });
    const { rerender } = render(<Verbindungstest ziel={ziel} passwort="" onZertifikat={() => {}} />);
    await userEvent.click(knopf());
    expect(await screen.findByRole("status")).toBeInTheDocument();
    rerender(<Verbindungstest ziel={{ ...ziel, pfad: "/anders" }} passwort="" onZertifikat={() => {}} />);
    expect(screen.queryByRole("status")).toBeNull();
  });
});

describe("Verbindung testen im Endpunkt-Editor", () => {
  it("nur mit Passwort-Eingabe (Ziel); Vertrauen setzt das Zertifikat am Endpunkt", async () => {
    rustAttrappe({
      passwort_vorhanden: () => false,
      verbindung_testen: () => {
        throw { kategorie: "zertifikat", meldung: "nicht anerkannt", fingerprint: FP };
      },
    });
    let zuletzt: Endpunkt = ziel;
    mitQuery(
      <Zustand<Endpunkt>
        start={ziel}
        beobachter={(w) => (zuletzt = w)}
        kind={(w, s) => <EndpunktFelder wert={w} ort="z" onChange={s} passwort={{ wert: "pw", onChange: () => {} }} />}
      />,
    );
    await userEvent.click(knopf());
    await userEvent.click(await screen.findByRole("button", { name: "Diesem Zertifikat vertrauen" }));
    expect(zuletzt).toEqual({ ...ziel, zertifikat: FP });
    expect(screen.getByText(/Bestätigtes Zertifikat/)).toBeInTheDocument();
  });
});
