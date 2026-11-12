import { useEffect, useState, type FormEvent } from "react";
import { api, type Settings } from "../api";
import { money, parseDecimal, parseMoney } from "../format";
import { Button, Field } from "../ui";
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
      <div className="form-row">
        <Field label="Date d'achat">{(id) => <input id={id} type="date" value={f.purchase_date} onChange={set("purchase_date")} />}</Field>
        <Field label={`Prix d'achat (${f.currency})`}>{(id) => <input id={id} inputMode="decimal" value={f.purchase_price_cents} onChange={set("purchase_price_cents")} />}</Field>
      </div>
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
      <Field label={`Électricité à domicile (${f.currency}/kWh)`} hint="Pré-remplit le coût des recharges « Maison ».">
        {(id) => <input id={id} inputMode="decimal" value={f.home_price_cents_per_kwh} onChange={set("home_price_cents_per_kwh")} required />}
      </Field>

      {msg && <div className={msg.ok ? "muted" : "form-error"} role="status">{msg.text}</div>}
      <Button type="submit" disabled={busy}>Enregistrer</Button>
      <Button variant="danger" onClick={logout}>Se déconnecter</Button>
    </form>
    </div>
  );
}
