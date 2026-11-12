import { useEffect, useState, type FormEvent } from "react";
import { api, type Settings } from "../api";
import { money, parseDecimal, parseMoney } from "../format";
import { Button, Field, Segmented } from "../ui";
import TeslaSection from "../components/TeslaSection";

type Form = Record<keyof Settings, string>;
const toForm = (s: Settings): Form => ({
  vehicle_name: s.vehicle_name,
  purchase_date: s.purchase_date ?? "",
  purchase_price_cents: s.purchase_price_cents == null ? "" : money(s.purchase_price_cents).replace(/\s/g, ""),
  initial_odometer_km: String(s.initial_odometer_km),
  current_odometer_km: s.current_odometer_km == null ? "" : String(s.current_odometer_km),
  currency: s.currency,
  fuel_consumption_l_per_100km: String(s.fuel_consumption_l_per_100km),
  fuel_price_cents_per_l: money(s.fuel_price_cents_per_l),
  home_price_cents_per_kwh: money(s.home_price_cents_per_kwh),
  acquisition_type: s.acquisition_type,
  leasing_down_payment_cents: s.leasing_down_payment_cents == null ? "" : money(s.leasing_down_payment_cents).replace(/\s/g, ""),
  leasing_monthly_cents: s.leasing_monthly_cents == null ? "" : money(s.leasing_monthly_cents).replace(/\s/g, ""),
  leasing_interest_rate: s.leasing_interest_rate == null ? "" : String(s.leasing_interest_rate),
  leasing_start_date: s.leasing_start_date ?? "",
  leasing_months: s.leasing_months == null ? "" : String(s.leasing_months),
  leasing_residual_cents: s.leasing_residual_cents == null ? "" : money(s.leasing_residual_cents).replace(/\s/g, ""),
  leasing_km_per_year: s.leasing_km_per_year == null ? "" : String(s.leasing_km_per_year),
  leasing_excess_cents_per_km: s.leasing_excess_cents_per_km == null ? "" : money(s.leasing_excess_cents_per_km),
  estimated_value_cents: s.estimated_value_cents == null ? "" : money(s.estimated_value_cents).replace(/\s/g, ""),
  estimate_home_charging: s.estimate_home_charging ? "true" : "false",
  consumption_kwh_per_100km: String(s.consumption_kwh_per_100km),
  charging_losses_pct: String(s.charging_losses_pct),
  monthly_budget_cents: s.monthly_budget_cents == null ? "" : money(s.monthly_budget_cents).replace(/\s/g, ""),
});

export default function SettingsPage({ onLogout }: { onLogout: () => void }) {
  const [f, setF] = useState<Form | null>(null);
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    api.settings().then((s) => setF(toForm(s))).catch((e) => setMsg({ ok: false, text: e.message }));
  }, []);
  if (!f) return msg ? <div className="empty">{msg.text}</div> : null;

  const set = (k: keyof Form) => (e: { target: { value: string } }) => setF({ ...f, [k]: e.target.value });
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setMsg(null);
    const num = (s: string) => { const v = parseDecimal(s); if (v === null || Number.isNaN(v)) throw new Error("Valeur numérique invalide"); return v; };
    const cents = (s: string) => { const v = parseMoney(s); if (v === null || Number.isNaN(v)) throw new Error("Montant invalide"); return v; };
    const opt = <T,>(v: string, fn: (v: string) => T): T | null => (v.trim() ? fn(v) : null);
    try {
      const s: Settings = {
        vehicle_name: f.vehicle_name,
        purchase_date: f.purchase_date || null,
        purchase_price_cents: f.purchase_price_cents.trim() ? cents(f.purchase_price_cents) : null,
        initial_odometer_km: Math.round(num(f.initial_odometer_km)),
        current_odometer_km: f.current_odometer_km.trim() ? Math.round(num(f.current_odometer_km)) : null,
        currency: f.currency.trim().toUpperCase(),
        fuel_consumption_l_per_100km: num(f.fuel_consumption_l_per_100km),
        fuel_price_cents_per_l: cents(f.fuel_price_cents_per_l),
        home_price_cents_per_kwh: cents(f.home_price_cents_per_kwh),
        acquisition_type: f.acquisition_type as Settings["acquisition_type"],
        leasing_down_payment_cents: opt(f.leasing_down_payment_cents, cents),
        leasing_monthly_cents: opt(f.leasing_monthly_cents, cents),
        leasing_interest_rate: opt(f.leasing_interest_rate, num),
        leasing_start_date: f.leasing_start_date || null,
        leasing_months: opt(f.leasing_months, (v) => Math.round(num(v))),
        leasing_residual_cents: opt(f.leasing_residual_cents, cents),
        leasing_km_per_year: opt(f.leasing_km_per_year, (v) => Math.round(num(v))),
        leasing_excess_cents_per_km: opt(f.leasing_excess_cents_per_km, cents),
        estimated_value_cents: opt(f.estimated_value_cents, cents),
        estimate_home_charging: f.estimate_home_charging === "true",
        consumption_kwh_per_100km: num(f.consumption_kwh_per_100km),
        charging_losses_pct: num(f.charging_losses_pct),
        monthly_budget_cents: opt(f.monthly_budget_cents, cents),
      };
      setBusy(true);
      setF(toForm(await api.saveSettings(s)));
      setMsg({ ok: true, text: "Réglages enregistrés" });
    } catch (err) {
      setMsg({ ok: false, text: (err as Error).message });
    } finally {
      setBusy(false);
    }
  };
  const logout = async () => {
    await api.logout().catch(() => {});
    onLogout();
  };

  return (
    <div style={{ maxWidth: 520, display: "flex", flexDirection: "column", gap: 40 }}>
    <TeslaSection />
    <form className="form" onSubmit={submit}>
      <h2 className="overline" style={{ margin: "8px 0 0" }}>Véhicule</h2>
      <Field label="Nom">{(id) => <input id={id} value={f.vehicle_name} onChange={set("vehicle_name")} required maxLength={100} />}</Field>
      <Segmented
        label="Mode d'acquisition"
        options={[{ value: "purchase", label: "Achat" }, { value: "leasing", label: "Leasing" }]}
        value={f.acquisition_type as "purchase" | "leasing"}
        onChange={(v) => setF({ ...f, acquisition_type: v })}
      />
      {f.acquisition_type === "purchase" ? (
        <div className="form-row">
          <Field label="Date d'achat">{(id) => <input id={id} type="date" value={f.purchase_date} onChange={set("purchase_date")} />}</Field>
          <Field label={`Prix d'achat (${f.currency})`}>{(id) => <input id={id} inputMode="decimal" value={f.purchase_price_cents} onChange={set("purchase_price_cents")} />}</Field>
        </div>
      ) : null}
      {f.acquisition_type === "purchase" ? (
        <Field label={`Valeur de revente estimée aujourd'hui (${f.currency})`} hint="Pour la dépréciation dans le coût de possession. Mets-la à jour de temps en temps (annonces, estimation en ligne).">
          {(id) => <input id={id} inputMode="decimal" value={f.estimated_value_cents} onChange={set("estimated_value_cents")} />}
        </Field>
      ) : (
        <LeasingFields f={f} set={set} />
      )}
      <div className="form-row">
        <Field label="Km à l'achat">{(id) => <input id={id} inputMode="numeric" value={f.initial_odometer_km} onChange={set("initial_odometer_km")} required />}</Field>
        <Field label="Km actuels" hint="Le plus haut entre ceci et tes saisies.">{(id) => <input id={id} inputMode="numeric" value={f.current_odometer_km} onChange={set("current_odometer_km")} />}</Field>
      </div>
      <Field label="Devise">{(id) => <input id={id} value={f.currency} onChange={set("currency")} maxLength={3} required />}</Field>

      <h2 className="overline" style={{ margin: "16px 0 0" }}>Comparatif essence</h2>
      <div className="form-row">
        <Field label="Consommation (L/100 km)" hint="D'une voiture essence équivalente.">{(id) => <input id={id} inputMode="decimal" value={f.fuel_consumption_l_per_100km} onChange={set("fuel_consumption_l_per_100km")} required />}</Field>
        <Field label={`Prix de l'essence (${f.currency}/L)`}>{(id) => <input id={id} inputMode="decimal" value={f.fuel_price_cents_per_l} onChange={set("fuel_price_cents_per_l")} required />}</Field>
      </div>
      <Field label={`Électricité à domicile (${f.currency}/kWh)`} hint="Pré-remplit le coût des recharges « Maison » et sert à l'estimation.">
        {(id) => <input id={id} inputMode="decimal" value={f.home_price_cents_per_kwh} onChange={set("home_price_cents_per_kwh")} required />}
      </Field>

      <h2 className="overline" style={{ margin: "16px 0 0" }}>Recharge à domicile</h2>
      <Segmented
        label="Recharges à la maison"
        hint={f.estimate_home_charging === "true"
          ? "Volta calcule l'énergie nécessaire d'après les km parcourus et compte la différence avec tes recharges saisies comme recharge maison."
          : "Tu saisis toi-même chaque recharge à la maison."}
        options={[{ value: "false", label: "Saisies" }, { value: "true", label: "Estimées" }]}
        value={f.estimate_home_charging as "true" | "false"}
        onChange={(v) => setF({ ...f, estimate_home_charging: v })}
      />
      {f.estimate_home_charging === "true" && (
        <div className="form-row">
          <Field label="Consommation (kWh/100 km)" hint="Visible dans les statistiques de la voiture.">{(id) => <input id={id} inputMode="decimal" value={f.consumption_kwh_per_100km} onChange={set("consumption_kwh_per_100km")} required />}</Field>
          <Field label="Pertes de recharge (%)" hint="Environ 10 % en courant alternatif.">{(id) => <input id={id} inputMode="decimal" value={f.charging_losses_pct} onChange={set("charging_losses_pct")} required />}</Field>
        </div>
      )}

      <h2 className="overline" style={{ margin: "16px 0 0" }}>Budget</h2>
      <Field label={`Budget mensuel (${f.currency})`} hint="Tout compris : recharges, dépenses, frais fixes. Vide = pas de suivi.">
        {(id) => <input id={id} inputMode="decimal" value={f.monthly_budget_cents} onChange={set("monthly_budget_cents")} />}
      </Field>

      {msg && <div className={msg.ok ? "muted" : "form-error"} role="status">{msg.text}</div>}
      <Button type="submit" disabled={busy}>Enregistrer</Button>
      <Button variant="danger" onClick={logout}>Se déconnecter</Button>
    </form>
    </div>
  );
}

function LeasingFields({ f, set }: { f: Form; set: (k: keyof Form) => (e: { target: { value: string } }) => void }) {
  const down = parseMoney(f.leasing_down_payment_cents) ?? 0;
  const monthly = parseMoney(f.leasing_monthly_cents);
  const months = parseDecimal(f.leasing_months);
  const total = monthly && months && !Number.isNaN(monthly) && !Number.isNaN(months) && !Number.isNaN(down) ? down + monthly * Math.round(months) : null;
  return (
    <>
      <div className="form-row">
        <Field label={`Acompte (${f.currency})`}>{(id) => <input id={id} inputMode="decimal" value={f.leasing_down_payment_cents} onChange={set("leasing_down_payment_cents")} />}</Field>
        <Field label={`Mensualité (${f.currency})`}>{(id) => <input id={id} inputMode="decimal" value={f.leasing_monthly_cents} onChange={set("leasing_monthly_cents")} required />}</Field>
      </div>
      <div className="form-row">
        <Field label="Début du contrat">{(id) => <input id={id} type="date" value={f.leasing_start_date} onChange={set("leasing_start_date")} required />}</Field>
        <Field label="Durée (mois)">{(id) => <input id={id} inputMode="numeric" value={f.leasing_months} onChange={set("leasing_months")} required />}</Field>
      </div>
      <Field label="Taux d'intérêt annuel (%)" hint="Informatif : il est déjà compris dans la mensualité.">
        {(id) => <input id={id} inputMode="decimal" value={f.leasing_interest_rate} onChange={set("leasing_interest_rate")} />}
      </Field>
      <div className="form-row">
        <Field label="Km inclus par an">{(id) => <input id={id} inputMode="numeric" value={f.leasing_km_per_year} onChange={set("leasing_km_per_year")} placeholder="15 000" />}</Field>
        <Field label={`Prix du km en plus (${f.currency})`}>{(id) => <input id={id} inputMode="decimal" value={f.leasing_excess_cents_per_km} onChange={set("leasing_excess_cents_per_km")} placeholder="0.15" />}</Field>
      </div>
      <Field label={`Valeur résiduelle (${f.currency})`} hint="Prix de rachat en fin de contrat.">
        {(id) => <input id={id} inputMode="decimal" value={f.leasing_residual_cents} onChange={set("leasing_residual_cents")} />}
      </Field>
      {total !== null && (
        <div className="muted">Coût total du contrat : {f.currency} {money(total)} (acompte + {Math.round(months!)} mensualités). Compté mois par mois dans les dépenses.</div>
      )}
    </>
  );
}
