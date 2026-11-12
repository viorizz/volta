import { useMemo, useState } from "react";
import { api, type ChargeInput, type Network } from "../api";
import { NETWORKS, parseDecimal } from "../format";
import { Button, Field } from "../ui";

/** Parseur CSV minimal : guillemets, séparateur `,` ou `;` détecté sur l'en-tête. */
export function parseCsv(text: string): string[][] {
  const firstLine = text.split(/\r?\n/, 1)[0] ?? "";
  const sep = (firstLine.match(/;/g)?.length ?? 0) > (firstLine.match(/,/g)?.length ?? 0) ? ";" : ",";
  const rows: string[][] = [];
  let row: string[] = [], cell = "", quoted = false;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (quoted) {
      if (c === '"' && text[i + 1] === '"') { cell += '"'; i++; }
      else if (c === '"') quoted = false;
      else cell += c;
    } else if (c === '"') quoted = true;
    else if (c === sep) { row.push(cell); cell = ""; }
    else if (c === "\n" || c === "\r") {
      if (c === "\r" && text[i + 1] === "\n") i++;
      row.push(cell); cell = "";
      if (row.some((x) => x.trim())) rows.push(row);
      row = [];
    } else cell += c;
  }
  row.push(cell);
  if (row.some((x) => x.trim())) rows.push(row);
  return rows;
}

/** Dates ISO, "31.10.2026 18:05", "31/10/2026". */
export function parseDate(s: string): Date | null {
  const t = s.trim();
  const m = t.match(/^(\d{1,2})[./](\d{1,2})[./](\d{4})(?:[ T,]+(\d{1,2}):(\d{2}))?/);
  if (m) return new Date(+m[3], +m[2] - 1, +m[1], +(m[4] ?? 12), +(m[5] ?? 0));
  const d = new Date(t);
  return Number.isNaN(d.getTime()) ? null : d;
}

type Col = "date" | "energy" | "cost" | "location" | "id";
const COLS: { key: Col; label: string; guess: RegExp; required: boolean }[] = [
  { key: "date", label: "Date", guess: /date|start|début|time/i, required: true },
  { key: "energy", label: "Énergie (kWh)", guess: /kwh|energy|énergie|energie|quantity|qty/i, required: true },
  { key: "cost", label: "Coût total", guess: /total|cost|amount|montant|prix|price/i, required: true },
  { key: "location", label: "Lieu", guess: /site|location|lieu|station|name/i, required: false },
  { key: "id", label: "Identifiant de session", guess: /session|invoice|facture|\bid\b/i, required: false },
];

export default function CsvImport({ onDone }: { onDone: () => void }) {
  const [rows, setRows] = useState<string[][] | null>(null);
  const [map, setMap] = useState<Record<Col, number>>({ date: -1, energy: -1, cost: -1, location: -1, id: -1 });
  const [network, setNetwork] = useState<Network>("supercharger");
  const [result, setResult] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const header = rows?.[0] ?? [];
  const onFile = async (file: File) => {
    setResult(null);
    setError(null);
    const parsed = parseCsv(await file.text());
    if (parsed.length < 2) return setError("Fichier vide ou illisible");
    setRows(parsed);
    const h = parsed[0];
    const next = {} as Record<Col, number>;
    for (const c of COLS) next[c.key] = h.findIndex((x) => c.guess.test(x));
    setMap(next);
  };

  const converted = useMemo(() => {
    if (!rows) return { ok: [] as ChargeInput[], bad: 0 };
    const ok: ChargeInput[] = [];
    let bad = 0;
    for (const r of rows.slice(1)) {
      const date = parseDate(r[map.date] ?? "");
      const energy = parseDecimal((r[map.energy] ?? "").replace(/[^\d.,-]/g, ""));
      const cost = parseDecimal((r[map.cost] ?? "").replace(/[^\d.,-]/g, ""));
      if (!date || energy === null || Number.isNaN(energy) || cost === null || Number.isNaN(cost)) { bad++; continue; }
      const id = map.id >= 0 ? r[map.id]?.trim() : "";
      ok.push({
        started_at: date.toISOString(),
        network,
        location: map.location >= 0 ? (r[map.location] ?? "").trim().slice(0, 200) : "",
        energy_kwh: energy,
        cost_cents: Math.round(cost * 100),
        odometer_km: null,
        notes: "",
        // Sans identifiant, une empreinte date+kWh+coût évite les doublons au réimport.
        external_id: id ? `csv:${id}` : `csv:${date.toISOString()}|${energy}|${cost}`,
      });
    }
    return { ok, bad };
  }, [rows, map, network]);

  const missing = COLS.filter((c) => c.required && map[c.key] < 0);
  const run = async () => {
    setBusy(true);
    setError(null);
    try {
      const r = await api.importCharges(converted.ok);
      setResult(`${r.inserted} recharge${r.inserted > 1 ? "s" : ""} importée${r.inserted > 1 ? "s" : ""}, ${r.skipped} déjà présente${r.skipped > 1 ? "s" : ""}.`);
      setRows(null);
      onDone();
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="form">
      <p className="muted" style={{ margin: 0 }}>
        Exporte ton historique de recharge en CSV, puis indique quelle colonne contient quoi. Réimporter le même fichier ne crée pas de doublons.
      </p>
      <Field label="Fichier CSV">
        {(id) => <input id={id} type="file" accept=".csv,text/csv" onChange={(e) => e.target.files?.[0] && onFile(e.target.files[0])} style={{ paddingTop: 12 }} />}
      </Field>
      {rows && (
        <>
          {COLS.map((c) => (
            <Field key={c.key} label={c.label + (c.required ? "" : " (optionnel)")}>
              {(id) => (
                <select id={id} value={map[c.key]} onChange={(e) => setMap({ ...map, [c.key]: +e.target.value })}>
                  <option value={-1}>—</option>
                  {header.map((h, i) => <option key={i} value={i}>{h || `Colonne ${i + 1}`}</option>)}
                </select>
              )}
            </Field>
          ))}
          <Field label="Réseau pour toutes les lignes">
            {(id) => (
              <select id={id} value={network} onChange={(e) => setNetwork(e.target.value as Network)}>
                {Object.entries(NETWORKS).map(([v, l]) => <option key={v} value={v}>{l}</option>)}
              </select>
            )}
          </Field>
          <div className="muted">
            {missing.length > 0
              ? `Colonne manquante : ${missing.map((m) => m.label).join(", ")}`
              : `${converted.ok.length} ligne${converted.ok.length > 1 ? "s" : ""} prête${converted.ok.length > 1 ? "s" : ""}${converted.bad ? `, ${converted.bad} illisible${converted.bad > 1 ? "s" : ""} ignorée${converted.bad > 1 ? "s" : ""}` : ""}.`}
          </div>
          <Button onClick={run} disabled={busy || missing.length > 0 || converted.ok.length === 0}>Importer</Button>
        </>
      )}
      {result && <div role="status">{result}</div>}
      {error && <div className="form-error" role="alert">{error}</div>}
    </div>
  );
}
