import type { Category, Network } from "./api";

/** Point décimal et espace fine pour les milliers : 1 284.50 (règle du design system). */
const nf = (min: number, max: number) => {
  const f = new Intl.NumberFormat("en-US", { minimumFractionDigits: min, maximumFractionDigits: max });
  return { format: (v: number) => f.format(v).replace(/,/g, "\u202f") };
};
const n0 = nf(0, 0);
const n1 = nf(0, 1);
const n2 = nf(2, 2);

/** 2180 → "21.80" (centimes → montant). */
export const money = (cents: number) => n2.format(cents / 100);
/** Montant arrondi au franc pour les grands chiffres. */
export const moneyRound = (cents: number) => n0.format(Math.round(cents / 100));
export const kwh = (v: number) => n1.format(v);
export const km = (v: number) => n0.format(v);

export const dateShort = (iso: string) =>
  new Date(iso.length === 10 ? iso + "T12:00:00" : iso).toLocaleDateString("fr-CH", { day: "numeric", month: "short", year: "numeric" });
export const monthLabel = (ym: string) =>
  new Date(ym + "-15T12:00:00").toLocaleDateString("fr-CH", { month: "long", year: "numeric" });
export const monthShort = (ym: string) =>
  new Date(ym + "-15T12:00:00").toLocaleDateString("fr-CH", { month: "short" });

/** "21,80" ou "21.80" → 2180 ; vide → null. */
export function parseMoney(s: string): number | null {
  const v = parseDecimal(s);
  return v === null ? null : Math.round(v * 100);
}
export function parseDecimal(s: string): number | null {
  const t = s.trim().replace(/[\s'’]/g, "").replace(",", ".");
  if (!t) return null;
  const v = Number(t);
  return Number.isFinite(v) ? v : NaN;
}

export const NETWORKS: Record<Network, string> = {
  supercharger: "Supercharger",
  home: "Maison",
  public: "Borne publique",
  other: "Autre",
};
export const CATEGORIES: Record<Category, string> = {
  tires: "Pneus",
  maintenance: "Entretien",
  insurance: "Assurance",
  tax: "Taxes & vignette",
  accessories: "Accessoires",
  parking: "Parking & péages",
  other: "Autre",
};

/** Date locale "YYYY-MM-DD" (pas en UTC). */
export function localDate(d = new Date()) {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}
/** Valeur pour <input type="datetime-local">. */
export function localDateTime(d = new Date()) {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${localDate(d)}T${p(d.getHours())}:${p(d.getMinutes())}`;
}
