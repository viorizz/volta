import { useState, type FormEvent } from "react";
import { api, type Charge, type Network, type Settings } from "../api";
import { NETWORKS, localDateTime, money, parseDecimal, parseMoney } from "../format";
import { Button, Field } from "../ui";

export default function ChargeForm({ charge, settings, onDone }: { charge?: Charge; settings: Settings | null; onDone: () => void }) {
  const [startedAt, setStartedAt] = useState(localDateTime(charge ? new Date(charge.started_at) : new Date()));
  const [network, setNetwork] = useState<Network>(charge?.network ?? "supercharger");
  const [location, setLocation] = useState(charge?.location ?? "");
  const [energy, setEnergy] = useState(charge ? String(charge.energy_kwh) : "");
  const [cost, setCost] = useState(charge ? money(charge.cost_cents).replace(/\s/g, "") : "");
  const [odometer, setOdometer] = useState(charge?.odometer_km != null ? String(charge.odometer_km) : "");
  const [notes, setNotes] = useState(charge?.notes ?? "");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const kwhValue = parseDecimal(energy);
  // À domicile, le coût peut être déduit du tarif de l'électricité.
  const homeCost = network === "home" && settings && kwhValue ? Math.round(kwhValue * settings.home_price_cents_per_kwh) : null;

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    const costCents = cost.trim() ? parseMoney(cost) : homeCost;
    const odo = odometer.trim() ? parseDecimal(odometer) : null;
    if (kwhValue === null || Number.isNaN(kwhValue)) return setError("Énergie (kWh) invalide");
    if (costCents === null || Number.isNaN(costCents)) return setError("Coût invalide");
    if (odo !== null && Number.isNaN(odo)) return setError("Kilométrage invalide");
    const input = {
      started_at: new Date(startedAt).toISOString(),
      network,
      location,
      energy_kwh: kwhValue,
      cost_cents: costCents,
      odometer_km: odo === null ? null : Math.round(odo),
      notes,
    };
    setBusy(true);
    try {
      if (charge) await api.updateCharge(charge.id, input);
      else await api.createCharge(input);
      onDone();
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setBusy(false);
    }
  };

  const remove = async () => {
    if (!charge || !confirm("Supprimer cette recharge ?")) return;
    setBusy(true);
    try {
      await api.deleteCharge(charge.id);
      onDone();
    } catch (err) {
      setError((err as Error).message);
      setBusy(false);
    }
  };

  const cur = settings?.currency ?? "CHF";
  return (
    <form className="form" onSubmit={submit}>
      <Field label="Réseau">
        {(id) => (
          <select id={id} value={network} onChange={(e) => setNetwork(e.target.value as Network)}>
            {Object.entries(NETWORKS).map(([v, l]) => <option key={v} value={v}>{l}</option>)}
          </select>
        )}
      </Field>
      <Field label="Date et heure">
        {(id) => <input id={id} type="datetime-local" value={startedAt} onChange={(e) => setStartedAt(e.target.value)} required />}
      </Field>
      <Field label="Lieu">
        {(id) => <input id={id} value={location} onChange={(e) => setLocation(e.target.value)} placeholder="Lausanne, Ionity Martigny…" maxLength={200} />}
      </Field>
      <div className="form-row">
        <Field label="Énergie (kWh)">
          {(id) => <input id={id} inputMode="decimal" value={energy} onChange={(e) => setEnergy(e.target.value)} required />}
        </Field>
        <Field label={`Coût (${cur})`} hint={homeCost !== null && !cost.trim() ? `Vide = ${cur} ${money(homeCost)} au tarif maison` : undefined}>
          {(id) => <input id={id} inputMode="decimal" value={cost} onChange={(e) => setCost(e.target.value)} required={homeCost === null} />}
        </Field>
      </div>
      <Field label="Kilométrage (optionnel)" hint="Sert au comparatif essence.">
        {(id) => <input id={id} inputMode="numeric" value={odometer} onChange={(e) => setOdometer(e.target.value)} />}
      </Field>
      <Field label="Notes">
        {(id) => <textarea id={id} value={notes} onChange={(e) => setNotes(e.target.value)} maxLength={2000} />}
      </Field>
      {error && <div className="form-error" role="alert">{error}</div>}
      <Button type="submit" disabled={busy}>{charge ? "Enregistrer" : "Ajouter la recharge"}</Button>
      {charge && <Button variant="danger" onClick={remove} disabled={busy}>Supprimer la recharge</Button>}
    </form>
  );
}
