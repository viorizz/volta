import { useState } from "react";
import { AlertTriangle } from "lucide-react";
import { api, type Stats } from "../api";
import { StatusBadge, dueText } from "./Maintenance";
import { useLoad } from "../useLoad";
import { CATEGORIES, FIXED_KINDS, NETWORKS, dateShort, km, kwh, localDate, money, moneyRound, tileMoney } from "../format";
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
  const { data: reminders } = useLoad(() => api.reminders());
  const due = reminders?.filter((r) => r.status !== "later") ?? [];

  return (
    <>
      {(s?.budget || due.length > 0) && (
        <section className="section">
          {s?.budget && <BudgetCard b={s.budget} currency={s.currency} />}
          {due.length > 0 && (
            <List>
              {due.map((r) => (
                <ListRow key={r.id} title={r.title} subtitle={dueText(r)} trailing={<StatusBadge status={r.status} />}
                  onClick={() => { location.hash = "entretien"; }} />
              ))}
            </List>
          )}
        </section>
      )}
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
              <StatTile label="Recharges" value={tileMoney(s.charge_cost_cents + s.comparison.estimated_home_cents)} unit={s.currency}
                note={`${kwh(s.energy_kwh + s.comparison.estimated_home_kwh)} kWh${s.comparison.estimated_home_cents > 0 ? " dont maison estimée" : ""}`} />
              <SavingsTile s={s} />
              <StatTile
                label="Coût aux 100 km"
                value={s.comparison.electric_per_100km_cents === null ? "—" : tileMoney(s.comparison.electric_per_100km_cents)}
                unit={s.currency}
                note={`Essence : ${s.currency} ${money(s.comparison.fuel_per_100km_cents)}`}
              />
              <StatTile label="Frais fixes" value={tileMoney(s.fixed_cents)} unit={s.currency} note={s.by_fixed.some((g) => g.key === "leasing") ? "Leasing, assurance, abonnements" : "Assurance, abonnements"} />
              <StatTile label="Autres dépenses" value={tileMoney(s.expense_cents)} unit={s.currency} note={`${s.expense_count} ligne${s.expense_count > 1 ? "s" : ""}`} />
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

          {s.comparison.estimated_home_cents > 0 && (
            <div className="muted" style={{ marginTop: -8, marginBottom: 24, fontSize: 13 }}>
              Électricité inclut {kwh(s.comparison.estimated_home_kwh)} kWh rechargés à domicile, estimés depuis le kilométrage ({s.currency} {money(s.comparison.estimated_home_cents)}).
            </div>
          )}

          {s.leasing && <LeasingCard l={s.leasing} currency={s.currency} />}
          {s.ownership && <OwnershipCard o={s.ownership} currency={s.currency} />}

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
      value={tileMoney(Math.abs(c.savings_cents))}
      unit={s.currency}
      tone={good ? "good" : "bad"}
      note={`${good ? "" : "Surcoût · "}essence ${s.currency} ${moneyRound(c.fuel_cost_cents)} · ${km(c.distance_km ?? 0)} km${c.distance_is_estimate ? " estimés" : ""}`}
    />
  );
}

function BudgetCard({ b, currency }: { b: NonNullable<Stats["budget"]>; currency: string }) {
  const ratio = b.budget_cents > 0 ? b.spent_cents / b.budget_cents : 0;
  const over = b.spent_cents > b.budget_cents;
  return (
    <div className="card">
      <div className="card-row">
        <span className="overline">Budget de {new Date(b.month + "-15").toLocaleDateString("fr-CH", { month: "long" })}</span>
        <span className={over ? "form-error" : "muted"}>
          {over ? `Dépassé de ${currency} ${money(b.spent_cents - b.budget_cents)}` : `Reste ${currency} ${money(b.budget_cents - b.spent_cents)}`}
        </span>
      </div>
      <div className="card-row"><span>{currency} {money(b.spent_cents)}</span><span className="muted">sur {money(b.budget_cents)}</span></div>
      <div className={`meter${over ? " is-over" : ""}`} role="meter" aria-valuenow={Math.round(ratio * 100)} aria-valuemin={0} aria-valuemax={100} aria-label="Budget utilisé">
        <span style={{ width: `${Math.min(100, ratio * 100)}%` }} />
      </div>
    </div>
  );
}

function LeasingCard({ l, currency }: { l: NonNullable<Stats["leasing"]>; currency: string }) {
  const ahead = l.km_driven - l.km_allowed_to_date;
  const excess = (l.projected_excess_km ?? 0) > 0;
  return (
    <section className="section">
      <div className="section-head"><h2>Leasing</h2><span className="overline">Fin {dateShort(l.end_date)}</span></div>
      <div className="grid">
        <StatTile label="Km parcourus" value={km(l.km_driven)} unit="km"
          note={`${ahead > 0 ? "+" : ""}${km(ahead)} km vs forfait à ce jour`} tone={ahead > 0 ? "bad" : "good"} />
        <StatTile label="Projection fin de contrat" value={l.projected_km === null ? "—" : km(l.projected_km)} unit="km"
          note={`Inclus : ${km(l.km_allowed_total)} km`} tone={excess ? "bad" : l.projected_km === null ? undefined : "good"} />
        <StatTile label="Km supplémentaires" value={excess ? tileMoney(l.projected_excess_cents ?? 0) : "0.00"} unit={currency}
          note={excess ? `${km(l.projected_excess_km!)} km au-delà du forfait` : "Au rythme actuel"} tone={excess ? "bad" : "good"} />
        {l.residual_cents !== null && <StatTile label="Valeur résiduelle" value={moneyRound(l.residual_cents)} unit={currency} note="Rachat possible en fin de contrat" />}
      </div>
    </section>
  );
}

function OwnershipCard({ o, currency }: { o: NonNullable<Stats["ownership"]>; currency: string }) {
  return (
    <section className="section">
      <div className="section-head"><h2>Coût de possession</h2><span className="overline">Depuis {dateShort(o.since)}</span></div>
      <div className="grid">
        <StatTile label="Par mois" value={tileMoney(o.per_month_cents)} unit={currency} note={`Sur ${o.months.toLocaleString("fr-CH")} mois`} />
        <StatTile label="Par km" value={o.per_km_cents === null ? "—" : tileMoney(o.per_km_cents)} unit={currency} note={o.total_km ? `${km(o.total_km)} km parcourus` : "Kilométrage manquant"} />
        <StatTile label="Total" value={moneyRound(o.total_cents)} unit={currency}
          note={o.depreciation_cents !== null ? `dont dépréciation ${currency} ${moneyRound(o.depreciation_cents)}` : "Sans dépréciation (leasing ou valeur inconnue)"} />
      </div>
    </section>
  );
}
