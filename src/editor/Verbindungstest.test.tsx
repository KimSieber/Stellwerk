import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import type { Endpunkt, SftpVerbindungstest, SmbVerbindungstest, Verbindungstest as Ergebnis } from "../api";
import { aufrufeVon, mitQuery, rustAttrappe, Zustand } from "../test/hilfen";
import { EndpunktFelder } from "./Felder";
import { SftpVerbindungstestKnopf, SmbVerbindungstestKnopf, Verbindungstest } from "./Verbindungstest";

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
    umbenennen_ueberschreibt: true,
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
    expect(ok).toHaveTextContent(/Umbenennenja$/);
    expect(knopf()).toHaveTextContent("Verbindung testen");
  });

  it("eingeschränkter Server über bestätigten Fingerprint", async () => {
    const e = ergebnis({
      begruessung: null,
      eintraege: 2,
      faehigkeiten: { mlsd: false, mfmt: false, chmod: null, fortsetzen: false, utf8: false },
      zertifikat: { fingerprint: FP, vom_system: false },
      umbenennen_ueberschreibt: null,
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
    expect(ok).toHaveTextContent("nein – Stellwerk merkt sich die Zeit selbst");
    expect(ok).toHaveTextContent("noch nicht bekannt – zeigt sich beim ersten Ersetzen");
    expect(ok).toHaveTextContent("unbekannt – wird beim ersten Setzen geprüft");
    expect(ok).toHaveTextContent(/REST\)nein/);
  });

  it("SITE CHMOD ausdrücklich nicht vorhanden", async () => {
    rustAttrappe({
      verbindung_testen: () => ergebnis({ faehigkeiten: { ...ergebnis().faehigkeiten, chmod: false }, umbenennen_ueberschreibt: false }),
    });
    render(<Verbindungstest ziel={ziel} passwort="" onZertifikat={() => {}} />);
    await userEvent.click(knopf());
    expect(await screen.findByRole("status")).toHaveTextContent(/CHMOD\)nein/);
    expect(screen.getByRole("status")).toHaveTextContent("nein – alte Datei wird vorher gelöscht (kurz fehlt sie)");
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

describe("Verbindung testen – Samba-Freigabe (v0.7.1b)", () => {
  const smb: Endpunkt & { typ: "smb" } = {
    typ: "smb", host: "192.168.1.11", port: 445, freigabe: "manitu", pfad: "/web", benutzer: "manitu", konto_ref: "stellwerk:s:z",
  };

  it("zeigt den Lauf und danach Ordner und Einträge", async () => {
    let fertig: (e: SmbVerbindungstest) => void = () => {};
    const a = rustAttrappe({ smb_verbindung_testen: () => new Promise<SmbVerbindungstest>((r) => (fertig = r)) });
    render(<SmbVerbindungstestKnopf ziel={smb} passwort="" />);
    await userEvent.click(knopf());
    expect(knopf()).toHaveTextContent("Teste Verbindung …");
    expect(knopf()).toBeDisabled();
    expect(aufrufeVon(a, "smb_verbindung_testen")[0].args).toMatchObject({ ziel: smb, passwort: null });
    fertig({ ordner: "smb://192.168.1.11:445/manitu/web", eintraege: 1 });
    const ok = await screen.findByRole("status");
    expect(ok).toHaveTextContent("✓ Verbunden mit 192.168.1.11");
    expect(ok).toHaveTextContent("Ordner smb://192.168.1.11:445/manitu/web · 1 Eintrag");
  });

  it("mehrere Einträge; eingegebenes Passwort wird mitgeschickt", async () => {
    const a = rustAttrappe({ smb_verbindung_testen: () => ({ ordner: "smb://x", eintraege: 3 }) });
    render(<SmbVerbindungstestKnopf ziel={smb} passwort="geheim" />);
    await userEvent.click(knopf());
    expect(await screen.findByRole("status")).toHaveTextContent("3 Einträge");
    expect(aufrufeVon(a, "smb_verbindung_testen")[0].args).toMatchObject({ passwort: "geheim" });
  });

  it("Fehler wird angezeigt", async () => {
    rustAttrappe({ smb_verbindung_testen: () => Promise.reject({ kategorie: "anmeldung", meldung: "Benutzer oder Passwort falsch" }) });
    render(<SmbVerbindungstestKnopf ziel={smb} passwort="" />);
    await userEvent.click(knopf());
    expect(await screen.findByText(/✗ .*Benutzer oder Passwort falsch/)).toBeInTheDocument();
  });

  it("ohne Server oder Freigabe nicht klickbar; im Editor nur mit Passwortfeld", () => {
    const { rerender } = render(<SmbVerbindungstestKnopf ziel={{ ...smb, host: " " }} passwort="" />);
    expect(knopf()).toBeDisabled();
    rerender(<SmbVerbindungstestKnopf ziel={{ ...smb, freigabe: "" }} passwort="" />);
    expect(knopf()).toBeDisabled();
    rerender(<EndpunktFelder wert={smb} ort="z" onChange={() => {}} />);
    expect(screen.queryByRole("button", { name: "Verbindung testen" })).toBeNull();
  });

  it("im Editor mit Passwortfeld vorhanden", async () => {
    rustAttrappe({ passwort_vorhanden: () => true });
    mitQuery(<EndpunktFelder wert={smb} ort="z" onChange={() => {}} passwort={{ wert: "", onChange: () => {} }} />);
    expect(await screen.findByRole("button", { name: "Verbindung testen" })).toBeEnabled();
  });
});

describe("Verbindung testen – SFTP-Server (v0.7.2)", () => {
  const sftp: Endpunkt & { typ: "sftp" } = {
    typ: "sftp", host: "ngcobalt418.manitu.net", port: 23, pfad: "/web", benutzer: "kim", konto_ref: "stellwerk:s:z",
  };
  const HK = `SHA256:${"A".repeat(43)}`;
  const ok = (teil: Partial<SftpVerbindungstest> = {}): SftpVerbindungstest => ({
    server: "SSH-2.0-OpenSSH_9.6", verzeichnis: "/web", eintraege: 1, hostschluessel: HK, ...teil,
  });
  const unbekannt = { kategorie: "host_schluessel", meldung: "noch nicht bestätigt", fingerprint: HK };

  it("zeigt den Lauf und danach Serverpfad, Einträge und Server-Software", async () => {
    let fertig: (e: SftpVerbindungstest) => void = () => {};
    const a = rustAttrappe({ sftp_verbindung_testen: () => new Promise<SftpVerbindungstest>((r) => (fertig = r)) });
    render(<SftpVerbindungstestKnopf ziel={{ ...sftp, hostschluessel: HK }} passwort="" onHostschluessel={() => {}} />);
    await userEvent.click(knopf());
    expect(knopf()).toHaveTextContent("Teste Verbindung …");
    expect(knopf()).toBeDisabled();
    expect(aufrufeVon(a, "sftp_verbindung_testen")[0].args).toMatchObject({ ziel: { ...sftp, hostschluessel: HK }, passwort: null });
    fertig(ok());
    const status = await screen.findByRole("status");
    expect(status).toHaveTextContent("✓ Verbunden mit ngcobalt418.manitu.net");
    expect(status).toHaveTextContent("Serverpfad /web · 1 Eintrag");
    expect(status).toHaveTextContent("SSH-2.0-OpenSSH_9.6");
  });

  it("mehrere Einträge, ohne Server-Kennung; eingegebenes Passwort wird mitgeschickt", async () => {
    const a = rustAttrappe({ sftp_verbindung_testen: () => ok({ eintraege: 3, server: null }) });
    render(<SftpVerbindungstestKnopf ziel={sftp} passwort="geheim" onHostschluessel={() => {}} />);
    await userEvent.click(knopf());
    const status = await screen.findByRole("status");
    expect(status).toHaveTextContent("3 Einträge");
    expect(status).not.toHaveTextContent("SSH-2.0");
    expect(aufrufeVon(a, "sftp_verbindung_testen")[0].args).toMatchObject({ passwort: "geheim" });
  });

  it("unbekannter Host-Schlüssel: bestätigen setzt ihn und testet sofort erneut", async () => {
    let n = 0;
    const a = rustAttrappe({
      sftp_verbindung_testen: () => {
        if (n++ === 0) throw unbekannt;
        return ok();
      },
    });
    let zuletzt: Endpunkt = sftp;
    mitQuery(
      <Zustand<Endpunkt>
        start={sftp}
        beobachter={(w) => (zuletzt = w)}
        kind={(w, s) => <EndpunktFelder wert={w} ort="z" onChange={s} passwort={{ wert: "pw", onChange: () => {} }} />}
      />,
    );
    await userEvent.click(knopf());
    expect(await screen.findByText("Unbekannter Host-Schlüssel")).toBeInTheDocument();
    expect(screen.getByText(HK)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Host-Schlüssel bestätigen" }));
    expect(zuletzt).toEqual({ ...sftp, hostschluessel: HK });
    expect(await screen.findByRole("status")).toHaveTextContent("✓ Verbunden");
    expect(aufrufeVon(a, "sftp_verbindung_testen")[1].args).toMatchObject({ ziel: { hostschluessel: HK } });
    expect(screen.getByText(/Bestätigter Host-Schlüssel/)).toBeInTheDocument();
  });

  it("geänderter Host-Schlüssel: deutliche Warnung", async () => {
    const neu = `SHA256:${"B".repeat(43)}`;
    rustAttrappe({ sftp_verbindung_testen: () => Promise.reject({ ...unbekannt, fingerprint: neu }) });
    const gesetzt: (string | undefined)[] = [];
    render(<SftpVerbindungstestKnopf ziel={{ ...sftp, hostschluessel: HK }} passwort="" onHostschluessel={(h) => gesetzt.push(h)} />);
    await userEvent.click(knopf());
    expect(await screen.findByText("⚠ Der Host-Schlüssel hat sich geändert")).toBeInTheDocument();
    expect(screen.getByText(/auch ein Angriff/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Neuen Host-Schlüssel bestätigen" }));
    expect(gesetzt).toEqual([neu]);
  });

  it("Zurücksetzen entfernt den bestätigten Schlüssel; andere Fehler als Text", async () => {
    rustAttrappe({ sftp_verbindung_testen: () => Promise.reject({ kategorie: "anmeldung", meldung: "Benutzer oder Passwort falsch" }) });
    const gesetzt: (string | undefined)[] = [];
    render(<SftpVerbindungstestKnopf ziel={{ ...sftp, hostschluessel: HK }} passwort="" onHostschluessel={(h) => gesetzt.push(h)} />);
    await userEvent.click(screen.getByRole("button", { name: "Zurücksetzen" }));
    expect(gesetzt).toEqual([undefined]);
    await userEvent.click(knopf());
    expect(await screen.findByText(/✗ .*Benutzer oder Passwort falsch/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /bestätigen/ })).toBeNull();
  });

  it("ohne Server nicht klickbar; im Editor nur mit Passwortfeld", () => {
    const { rerender } = render(<SftpVerbindungstestKnopf ziel={{ ...sftp, host: " " }} passwort="" onHostschluessel={() => {}} />);
    expect(knopf()).toBeDisabled();
    rerender(<EndpunktFelder wert={sftp} ort="z" onChange={() => {}} />);
    expect(screen.queryByRole("button", { name: "Verbindung testen" })).toBeNull();
  });
});
