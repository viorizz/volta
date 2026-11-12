import { useEffect, useRef, useState } from "react";
import type { Month } from "../api";
import { money, moneyRound, monthLabel, monthShort } from "../format";

/**
 * Barres groupées par mois : coût de recharge (accent) face à l'équivalent
 * essence estimé (chart-fuel). Une seule échelle, légende, survol, vue tableau.
 */
export default function CompareChart({ months, currency }: { months: Month[]; currency: string }) {
  const [active, setActive] = useState<number | null>(null);
  const [table, setTable] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  const [W, setW] = useState(640);
  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const ro = new ResizeObserver(([e]) => setW(Math.max(280, Math.round(e.contentRect.width))));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  // Uniquement les mois avec des recharges, sur une plage continue (mois vides = 0).
  const data = continuousMonths(months.filter((m) => m.energy_kwh > 0)).slice(W < 520 ? -6 : -12);
  if (data.length === 0) return null;

  const H = W < 520 ? 200 : 240, padL = 44, padB = 24, padT = 8;
  const max = Math.max(1, ...data.flatMap((m) => [m.charge_cost_cents, m.fuel_equivalent_cents ?? 0]));
  const step = niceStep(max / 4);
  const top = Math.ceil(max / step) * step;
  const y = (v: number) => padT + (H - padT - padB) * (1 - v / top);
  const slot = (W - padL) / data.length;
  const barW = Math.min(18, (slot - 10) / 2);
  const ticks = Array.from({ length: Math.round(top / step) + 1 }, (_, i) => i * step);
  const a = active !== null ? data[active] : null;

  return (
    <div className="chart" ref={box}>
      <div className="section-head" style={{ marginBottom: 12 }}>
        <div className="legend">
          <span style={{ ["--c" as string]: "var(--accent)" }}>Électricité</span>
          <span style={{ ["--c" as string]: "var(--chart-fuel)" }}>Essence (équivalent estimé)</span>
        </div>
        <button className="vt-btn vt-btn-ghost vt-btn-sm" onClick={() => setTable((t) => !t)}>
          {table ? "Graphique" : "Tableau"}
        </button>
      </div>

      {table ? (
        <table className="table">
          <thead>
            <tr><th>Mois</th><th>Électricité ({currency})</th><th>Essence ({currency})</th></tr>
          </thead>
          <tbody>
            {data.map((m) => (
              <tr key={m.month}>
                <td>{monthLabel(m.month)}</td>
                <td>{money(m.charge_cost_cents)}</td>
                <td>{m.fuel_equivalent_cents === null ? "—" : money(m.fuel_equivalent_cents)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : (
        <>
          <svg viewBox={`0 0 ${W} ${H}`} width={W} height={H} role="img" aria-label="Coût mensuel de l'électricité comparé à l'essence" onMouseLeave={() => setActive(null)}>
            {ticks.map((t) => (
              <g key={t}>
                <line className="grid-line" x1={padL} x2={W} y1={y(t)} y2={y(t)} />
                <text className="axis" x={padL - 8} y={y(t) + 4} textAnchor="end">{moneyRound(t)}</text>
              </g>
            ))}
            {data.map((m, i) => {
              const x0 = padL + i * slot;
              const cx = x0 + slot / 2;
              return (
                <g key={m.month}>
                  <rect className={`hit${active === i ? " is-active" : ""}`} x={x0 + 2} y={padT} width={slot - 4} height={H - padT - padB} rx={4}
                    onMouseEnter={() => setActive(i)} onClick={() => setActive(i)} />
                  <Bar className="bar-electric" x={cx - barW - 1} w={barW} y={y(m.charge_cost_cents)} base={y(0)} />
                  {m.fuel_equivalent_cents !== null && <Bar className="bar-fuel" x={cx + 1} w={barW} y={y(m.fuel_equivalent_cents)} base={y(0)} />}
                  <text className="axis" x={cx} y={H - 6} textAnchor="middle">{monthShort(m.month)}</text>
                </g>
              );
            })}
          </svg>
          {a && active !== null && (
            <div className="tooltip" style={{ left: `calc(${((padL + (active + 0.5) * slot) / W) * 100}% )`, top: 52 }}>
              <b>{monthLabel(a.month)}</b>
              <div>Électricité : {currency} {money(a.charge_cost_cents)}</div>
              <div>Essence : {a.fuel_equivalent_cents === null ? "—" : `${currency} ${money(a.fuel_equivalent_cents)}`}</div>
            </div>
          )}
        </>
      )}
    </div>
  );
}

/** Barre à bout supérieur arrondi (4px), ancrée sur la ligne de base. */
function Bar({ className, x, w, y, base }: { className: string; x: number; w: number; y: number; base: number }) {
  const h = Math.max(0, base - y);
  if (h < 0.5) return null;
  const r = Math.min(4, h, w / 2);
  const d = `M${x},${base} V${y + r} Q${x},${y} ${x + r},${y} H${x + w - r} Q${x + w},${y} ${x + w},${y + r} V${base} Z`;
  return <path className={className} d={d} />;
}

function continuousMonths(ms: Month[]): Month[] {
  if (ms.length === 0) return [];
  const byKey = new Map(ms.map((m) => [m.month, m]));
  const out: Month[] = [];
  let [y, mo] = ms[0].month.split("-").map(Number);
  const last = ms[ms.length - 1].month;
  for (let guard = 0; guard < 240; guard++) {
    const key = `${y}-${String(mo).padStart(2, "0")}`;
    out.push(byKey.get(key) ?? { month: key, charge_cost_cents: 0, energy_kwh: 0, expense_cents: 0, fixed_cents: 0, fuel_equivalent_cents: 0 });
    if (key === last) break;
    mo++;
    if (mo > 12) { mo = 1; y++; }
  }
  return out;
}

function niceStep(raw: number) {
  const p = Math.pow(10, Math.floor(Math.log10(raw)));
  const n = raw / p;
  return (n <= 1 ? 1 : n <= 2 ? 2 : n <= 5 ? 5 : 10) * p;
}
