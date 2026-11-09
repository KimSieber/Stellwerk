// Reine Hilfsfunktionen des Strecken-Editors (ohne Oberfläche, voll testbar).

import type { Endpunkt, Problem, Quelle, Regel, RegelTyp, SmbEndpunkt, Stufe, Strecke } from "../api";

/** Anzeigenamen der Regeltypen. */
export const REGEL_TYPEN: [RegelTyp, string][] = [
  ["ignorieren", "Ignorieren"],
  ["schuetzen", "Schützen"],
  ["nur_anlegen", "Nur anlegen"],
  ["ersetzen", "Ersetzen durch"],
];

/** Kennung aus einem Namen: klein, Umlaute umschrieben, nur a-z, 0-9 und „-“. */
export function kennungAus(name: string): string {
  return name
    .toLowerCase()
    .replace(/ä/g, "ae")
    .replace(/ö/g, "oe")
    .replace(/ü/g, "ue")
    .replace(/ß/g, "ss")
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 64)
    .replace(/-+$/, "");
}

/**
 * Gruppe zum Speichern: Rand-Leerzeichen weg, leer = ohne Gruppe (Feld entfällt).
 * Erst beim Speichern – sonst ließe sich kein Leerzeichen zwischen zwei Wörtern tippen.
 */
export function gruppeBereinigt(s: Strecke): Strecke {
  const { gruppe, ...rest } = s;
  const g = (gruppe ?? "").trim();
  return g === "" ? rest : { ...rest, gruppe: g };
}

/** Interner Name des Schlüsselbund-Eintrags einer Stufe – vergibt Stellwerk selbst. */
export function kontoRef(streckeId: string, stufeId: string): string {
  return `stellwerk:${streckeId}:${stufeId}`;
}

/** Schlüsselbund-Name einer Quelle auf einem Server (v0.6.5) – getrennt vom Ziel. */
export function kontoRefQuelle(streckeId: string, stufeId: string): string {
  return `${kontoRef(streckeId, stufeId)}:quelle`;
}

/** Erste freie Stufenkennung „stufe-N“. */
export function freieStufenId(strecke: Strecke): string {
  const vergeben = new Set(strecke.stufen.map((s) => s.id));
  let n = strecke.stufen.length + 1;
  while (vergeben.has(`stufe-${n}`)) n += 1;
  return `stufe-${n}`;
}

export function neueStufe(strecke: Strecke): Stufe {
  const id = freieStufenId(strecke);
  const vorige = strecke.stufen[strecke.stufen.length - 1];
  return {
    id,
    name: `Stufe ${strecke.stufen.length + 1}`,
    modus: "direkt",
    // Ab der zweiten Stufe wird der Zielordner der Vorstufe als Quellordner
    // vorgeschlagen (als normaler Pfad, frei änderbar – kein Verweis).
    quelle: { typ: "lokal", pfad: vorige?.ziel.typ === "lokal" ? vorige.ziel.pfad : "" },
    ziel: { typ: "lokal", pfad: "" },
    nur_online: "herunterladen",
  };
}

export function neueStrecke(): Strecke {
  const leer: Strecke = { id: "", name: "", version: 1, stufen: [] };
  return { ...leer, stufen: [neueStufe(leer)] };
}

/** Leerer Endpunkt des gewünschten Typs. */
export function endpunktVom(typ: Endpunkt["typ"], konto: string): Endpunkt {
  if (typ === "lokal") return { typ: "lokal", pfad: "" };
  if (typ === "smb") return { typ: "smb", host: "", port: 445, freigabe: "", pfad: "/", benutzer: "", konto_ref: konto };
  if (typ === "sftp") return { typ: "sftp", host: "", port: 22, pfad: "/", benutzer: "", konto_ref: konto };
  return { typ: "ftps", host: "", port: 21, pfad: "/", benutzer: "", tls: "explizit", konto_ref: konto };
}

/** Hat der Endpunkt ein Passwort im Schlüsselbund? (FTPS, Samba-Freigabe, SFTP) */
export function mitPasswort(e: Quelle): e is Endpunkt & { typ: "ftps" | "smb" | "sftp" } {
  return e.typ === "ftps" || e.typ === "smb" || e.typ === "sftp";
}

/** Adresse einer Freigabe wie im Finder: `smb://host[:port]/freigabe/ordner`. */
export function smbAdresse(e: SmbEndpunkt): string {
  if (!e.host && !e.freigabe) return "";
  const port = e.port === 445 ? "" : `:${e.port}`;
  const pfad = e.pfad === "/" ? "" : e.pfad.replace(/\/+$/, "");
  return `smb://${e.host}${port}/${e.freigabe}${pfad}`;
}

/** Liest eine Adresse (`smb://…`, auch ohne Präfix oder aus dem Finder mit `%20`) in die Felder. */
export function smbAusAdresse<T extends SmbEndpunkt>(text: string, e: T): T {
  let rest = text.trim().replace(/^smb:\/\//i, "");
  try {
    rest = decodeURIComponent(rest);
  } catch {
    // Unvollständige %-Folge beim Tippen: so lassen
  }
  const [server = "", freigabe = "", ...ordner] = rest.split("/");
  const m = /^(.*?)(?::(\d*))?$/.exec(server)!;
  const port = m[2] === undefined ? 445 : Number.parseInt(m[2], 10) || 0;
  const teile = ordner.filter((t) => t !== "");
  return { ...e, host: m[1], port, freigabe, pfad: `/${teile.join("/")}` };
}

/** Leere Quelle des gewünschten Typs; `stufe` verweist auf die Vorstufe, `ftps` erhält `konto` (v0.6.5). */
export function quelleVom(typ: Quelle["typ"], vorstufe?: string, konto = ""): Quelle {
  if (typ === "stufe") return { typ: "stufe", stufe: vorstufe ?? "" };
  return endpunktVom(typ, konto);
}

/** Setzt bei allen Server- und Samba-Zielen und -Quellen den internen Schlüsselbund-Namen passend zu Strecke und Stufe. */
export function kontenAngleichen(s: Strecke): Strecke {
  return {
    ...s,
    stufen: s.stufen.map((st) => {
      let neu = st;
      if (mitPasswort(neu.ziel)) neu = { ...neu, ziel: { ...neu.ziel, konto_ref: kontoRef(s.id, st.id) } };
      if (mitPasswort(neu.quelle)) neu = { ...neu, quelle: { ...neu.quelle, konto_ref: kontoRefQuelle(s.id, st.id) } };
      return neu;
    }),
  };
}

export function neueRegel(): Regel {
  return { muster: "", typ: "ignorieren" };
}

/** Probleme, deren Ort genau `ort` ist. */
export function problemeAn(probleme: Problem[], ort: string): Problem[] {
  return probleme.filter((p) => p.ort === ort);
}

export function hatFehler(probleme: Problem[]): boolean {
  return probleme.some((p) => p.schwere === "fehler");
}

/** Setzt den Ordner am Ort `stufen[i].quelle.pfad` bzw. `stufen[i].ziel.pfad`
 *  (Ziel einer Ablage per Drag-and-drop). Andere Orte oder Nicht-Ordner: unverändert. */
export function pfadSetzen(s: Strecke, ort: string, pfad: string): Strecke {
  const m = /^stufen\[(\d+)\]\.(quelle|ziel)\.pfad$/.exec(ort);
  const i = m ? Number(m[1]) : -1;
  const seite = m?.[2] as "quelle" | "ziel" | undefined;
  const st = s.stufen[i];
  if (!st || !seite || st[seite].typ !== "lokal") return s;
  return { ...s, stufen: s.stufen.map((x, k) => (k === i ? { ...x, [seite]: { typ: "lokal", pfad } } : x)) };
}

const ORT_NAMEN: Record<string, string> = {
  name: "Name",
  id: "Kennung",
  version: "Formatversion",
  stufen: "Stufe",
  regeln: "Regeln",
  eigene: "Regel",
  standard_aus: "Abschaltung",
  quelle: "Quelle",
  ziel: "Ziel",
  pfad: "Pfad",
  host: "Server",
  freigabe: "Freigabe",
  hostschluessel: "Host-Schlüssel",
  schluessel: "SSH-Schlüssel",
  port: "Port",
  benutzer: "Benutzer",
  konto_ref: "Passwort",
  passwort: "Passwort",
  typ: "Art",
  stufe: "Vorstufe",
  muster: "Muster",
  rechte: "Dateirechte",
  ordner: "Ordner",
  dateien: "Dateien",
  modus: "Modus",
  nur_online: "„nur online“",
  global: "Global",
};

/** So heißt das Feld „pfad“ je Art des Endpunkts. */
const PFAD_NAME: Record<Quelle["typ"], string> = {
  lokal: "Ordner",
  ftps: "Serverpfad",
  smb: "Ordner in der Freigabe",
  sftp: "Serverpfad",
  stufe: "Ordner",
};

/** Ort einer Meldung für Menschen: `stufen[1].ziel.pfad` → `Stufe 2 „Server“ › Ziel › Pfad`. */
export function ortLesbar(ort: string, s?: Strecke): string {
  const teile = ort.split(".").map((teil) => {
    const m = /^(\w+)\[(\d+)\]$/.exec(teil);
    const name = m ? m[1] : teil;
    const text = ORT_NAMEN[name] ?? name;
    if (!m) return text;
    const nr = Number(m[2]);
    const stufenName = name === "stufen" ? s?.stufen[nr]?.name : undefined;
    return stufenName ? `${text} ${nr + 1} „${stufenName}“` : `${text} ${nr + 1}`;
  });
  // Regeln auf oberster Ebene gehören zur Strecke
  if (ort.startsWith("regeln.")) teile[0] = "Regeln der Strecke";
  // „Pfad“ heißt am Feld „Ordner“ (lokal), „Serverpfad“ (FTPS) bzw. „Ordner in der Freigabe“ (SMB)
  const m = /^stufen\[(\d+)\]\.(quelle|ziel)\.pfad$/.exec(ort);
  const endpunkt = m ? s?.stufen[Number(m[1])]?.[m[2] as "quelle" | "ziel"] : undefined;
  if (endpunkt) teile[2] = PFAD_NAME[endpunkt.typ];
  return teile.join(" › ");
}

/** Regel einer übergeordneten Ebene, die auf der aktuellen Ebene abgeschaltet werden kann. */
export type ObereRegel = { muster: string; typ: RegelTyp; herkunft: "global" | "strecke" };

const wirksam = (r: Regel) => r.aktiv !== false && r.muster.trim() !== "";

/** Was auf Ebene „Strecke“ abschaltbar ist: die wirksamen globalen Regeln. */
export function regelnFuerStrecke(global: Regel[]): ObereRegel[] {
  return global.filter(wirksam).map((r) => ({ muster: r.muster, typ: r.typ, herkunft: "global" }));
}

/** Was auf Ebene „Stufe“ abschaltbar ist: globale Regeln, die die Strecke nicht abschaltet,
 *  und die wirksamen eigenen Regeln der Strecke. */
export function regelnFuerStufe(global: Regel[], s: Strecke): ObereRegel[] {
  const aus = new Set(s.regeln?.standard_aus ?? []);
  return [
    ...regelnFuerStrecke(global).filter((r) => !aus.has(r.muster)),
    ...(s.regeln?.eigene ?? []).filter(wirksam).map((r) => ({ muster: r.muster, typ: r.typ, herkunft: "strecke" as const })),
  ];
}

/** Schaltet eine übergeordnete Regel auf dieser Ebene an (aus Liste entfernen) oder aus. */
export function ausUmschalten(aus: string[], muster: string, an: boolean): string[] {
  const ohne = aus.filter((m) => m !== muster);
  return an ? ohne : [...ohne, muster];
}

/** Muster lesbar machen – z. B. das Steuerzeichen im macOS-Muster „Icon[\r]“. */
export function musterAnzeige(muster: string): string {
  return muster.replace(/\r/g, "\\r");
}
