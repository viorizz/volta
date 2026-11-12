import { useState } from "react";
import { AlertTriangle } from "lucide-react";
import { api } from "../api";
import { useLoad } from "../useLoad";
import { CATEGORIES, FIXED_KINDS, NETWORKS, km, kwh, localDate, money, moneyRound } from "../format";
import { List, ListRow, Segmented, StatTile } from "../ui";
import CompareChart from "../components/CompareChart";

type Period = "month" | "year" | "all";
const PERIODS: { value: Period; label: string }[] = [
  { value: "month", label: "Mois" },
  { value: "year", label: "Année" },
  { value: "all", label: "Tout" },
];
function fromDate(p: Period) {
  const d = new Date();
  if (p === "month") return localDate(new Date(d.getFullYear(), d.getMonth(), 1));
  if (p === "year") return localDate(new Date(d.getFullYear(), 0, 1));
  return undefined;
}

export default function Overview() {
  const [period, setPeriod] = useState<Period>("all");
  const { data: s, error } = useLoad(() => api.stats(fromDate(period)), [period]);

  return (
    <>
      <div style={{ maxWidth: 420 }}>
        <Segmented label="Période" hideLabel options={PERIODS} value={period} onChange={setPeriod} />
      </div>
      {error && <div className="notice is-error" style={{ marginTop: 16 }}><AlertTriangle size={20} />{error}</div>}
      {s && (
        <>
          <div className="hero">
            <div className="overline">{period === "all" ? "Dépensé depuis le début" : period === "year" ? "Dépensé cette année" : "Dépensé ce mois-ci"}</div>
            <div className="hero-value">{moneyRound(s.total_cost_cents)}<small>{s.currency}</small></div>
            <div className="muted">
              {s.charge_count} recharge{s.charge_count > 1 ? "s" : ""} · {s.expense_count} dépense{s.expense_count > 1 ? "s" : ""}{s.fixed_cents > 0 ? " · frais fixes inclus" : ""}
            </div>
            {period === "all" && s.purchase_price_cents ? (
              <div className="muted">Sans l'achat ({s.currency} {moneyRound(s.purchase_price_cents)})</div>
            ) : null}
          </div>

          <section className="section">
            <div className="grid">
              <StatTile label="Recharges" value={money(s.charge_cost_cents)} unit={s.currency} note={`${kwh(s.energy_kwh)} kWh`} />
              <SavingsTile s={s} />
              <StatTile
                label="Coût aux 100 km"
                value={s.comparison.electric_per_100km_cents === null ? "—" : money(s.comparison.electric_per_100km_cents)}
                unit={s.currency}
                note={`Essence : ${s.currency} ${money(s.comparison.fuel_per_100km_cents)}`}
              />
              <StatTile label="Frais fixes" value={money(s.fixed_cents)} unit={s.currency} note={s.by_fixed.some((g) => g.key === "leasing") ? "Leasing, assurance, abonnements" : "Assurance, abonnements"} />
              <StatTile label="Autres dépenses" value={money(s.expense_cents)} unit={s.currency} note={`${s.expense_count} ligne${s.expense_count > 1 ? "s" : ""}`} />
            </div>
            {s.comparison.km_per_kwh !== null && !s.comparison.efficiency_plausible && (
              <div className="notice">
                <AlertTriangle size={20} />
                <div>
                  <b>Comparatif peu fiable.</b>{" "}
                  <span className="muted">
                    Le compteur et tes recharges donnent {kwh(s.comparison.km_per_kwh)} km par kWh, alors qu'une voiture électrique fait entre 4 et 8.
                    Il manque sûrement des recharges (à domicile ?) ou le kilométrage initial est faux.
                  </span>
                </div>
              </div>
            )}
            {s.comparison.distance_km === null && (
              <div className="notice">
                <AlertTriangle size={20} />
                <span className="muted">Pour comparer avec l'essence, saisis un kilométrage (dans une recharge, une dépense ou les réglages).</span>
              </div>
            )}
          </section>

          {s.monthly.length > 0 && (
            <section className="section">
              <div className="section-head"><h2>Électricité ou essence, mois par mois</h2></div>
              <CompareChart months={s.monthly} currency={s.currency} />
            </section>
          )}

          {s.by_network.length > 0 && (
            <section className="section">
              <div className="section-head"><h2>Par réseau</h2></div>
              <List>
                {s.by_network.map((g) => (
                  <ListRow key={g.key} title={NETWORKS[g.key as keyof typeof NETWORKS] ?? g.key}
                    subtitle={`${g.count} recharge${g.count > 1 ? "s" : ""} · ${kwh(g.energy_kwh)} kWh`}
                    trailing={`${s.currency} ${money(g.cost_cents)}`} />
                ))}
              </List>
            </section>
          )}

          {s.by_fixed.length > 0 && (
            <section className="section">
              <div className="section-head"><h2>Frais fixes</h2></div>
              <List>
                {s.by_fixed.map((g) => (
                  <ListRow key={g.key} title={FIXED_KINDS[g.key as keyof typeof FIXED_KINDS] ?? g.key} trailing={`${s.currency} ${money(g.cost_cents)}`} />
                ))}
              </List>
            </section>
          )}

          {s.by_category.length > 0 && (
            <section className="section">
              <div className="section-head"><h2>Par catégorie</h2></div>
              <List>
                {s.by_category.map((g) => (
                  <ListRow key={g.key} title={CATEGORIES[g.key as keyof typeof CATEGORIES] ?? g.key}
                    subtitle={`${g.count} dépense${g.count > 1 ? "s" : ""}`} trailing={`${s.currency} ${money(g.cost_cents)}`} />
                ))}
              </List>
            </section>
          )}

          {s.charge_count === 0 && s.expense_count === 0 && s.fixed_cents === 0 && (
            <div className="empty">Rien pour cette période. Ajoute une recharge ou une dépense pour commencer.</div>
          )}
        </>
      )}
    </>
  );
}

function SavingsTile({ s }: { s: NonNullable<ReturnType<typeof useLoad<import("../api").Stats>>["data"]> }) {
  const c = s.comparison;
  if (c.savings_cents === null || c.fuel_cost_cents === null) {
    return <StatTile label="Économie vs essence" value="—" note="Kilométrage manquant" />;
  }
  const good = c.savings_cents >= 0;
  return (
    <StatTile
      label="Économie vs essence"
      value={money(Math.abs(c.savings_cents))}
      unit={s.currency}
      tone={good ? "good" : "bad"}
      note={`${good ? "" : "Surcoût · "}essence ${s.currency} ${moneyRound(c.fuel_cost_cents)} · ${km(c.distance_km ?? 0)} km${c.distance_is_estimate ? " estimés" : ""}`}
    />
  );
}
