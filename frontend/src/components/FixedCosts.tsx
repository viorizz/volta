import { useState, type FormEvent } from "react";
import { Plus } from "lucide-react";
import { api, type Coverage, type FixedCost, type FixedKind, type Settings } from "../api";
import { useLoad } from "../useLoad";
import { COVERAGES, FIXED_KINDS, FIXED_SHORT, dateShort, localDate, money, parseMoney } from "../format";
import { Badge, Button, Field, List, ListRow, Segmented, Sheet } from "../ui";

type Draft = Partial<FixedCost>;

/** Raccourcis : préremplissent le formulaire, tout reste modifiable. */
const PRESETS: { label: string; draft: Draft }[] = [
  { label: "Assurance", draft: { kind: "insurance", label: "Assurance", frequency: "yearly", coverage: "full" } },
  { label: "Conduite assistée · 99.–/mois", draft: { kind: "vehicle_option", label: "Conduite assistée", amount_cents: 9900, frequency: "monthly" } },
  { label: "Connectivité Premium · 12.99/mois", draft: { kind: "vehicle_option", label: "Connectivité Premium", amount_cents: 1299, frequency: "monthly" } },
  { label: "Joya · 11.90/mois", draft: { kind: "charging_subscription", label: "Joya", amount_cents: 1190, frequency: "monthly" } },
  { label: "Vignette · 40.–/an", draft: { kind: "tax", label: "Vignette autoroutière", amount_cents: 4000, frequency: "yearly" } },
  { label: "Impôt cantonal", draft: { kind: "tax", label: "Impôt cantonal sur les véhicules", frequency: "yearly" } },
];

const isActive = (f: FixedCost) => !f.end_date || f.end_date >= localDate();
const perMonth = (f: FixedCost) => (f.frequency === "yearly" ? f.amount_cents / 12 : f.amount_cents);

export default function FixedCosts() {
  const { data, error, reload } = useLoad(() => api.fixedCosts());
  const { data: settings } = useLoad(() => api.settings());
  const [editing, setEditing] = useState<Draft | FixedCost | null>(null);
  const cur = settings?.currency ?? "CHF";
  const active = data?.filter(isActive) ?? [];
  const ended = data?.filter((f) => !isActive(f)) ?? [];
  const leasingMonthly = settings?.acquisition_type === "leasing" ? settings.leasing_monthly_cents ?? 0 : 0;
  const monthly = active.reduce((s, f) => s + perMonth(f), 0) + leasingMonthly;

  const row = (f: FixedCost) => (
    <ListRow
      key={f.id}
      onClick={() => setEditing(f)}
      title={f.label}
      subtitle={`${f.coverage ? COVERAGES[f.coverage] + " · " : ""}${f.end_date ? `${dateShort(f.start_date)} → ${dateShort(f.end_date)}` : `depuis ${dateShort(f.start_date)}`}`}
      trailing={
        <>
          <div>{cur} {money(f.amount_cents)}<span className="muted"> /{f.frequency === "yearly" ? "an" : "mois"}</span></div>
          <Badge tone={f.kind === "charging_subscription" ? "electric" : undefined}>{FIXED_SHORT[f.kind]}</Badge>
        </>
      }
    />
  );

  return (
    <section className="section">
      <div className="section-head">
        <h2>Frais fixes</h2>
        {data && <span className="overline num">{cur} {money(Math.round(monthly))} / mois</span>}
      </div>
      {error && <div className="empty">{error}</div>}
      {settings?.acquisition_type === "leasing" && (
        <List>
          <ListRow title="Leasing" subtitle="Modifiable dans les Réglages" trailing={<div>{cur} {money(leasingMonthly)}<span className="muted"> /mois</span></div>} />
        </List>
      )}
      {active.length > 0 && <List>{active.map(row)}</List>}
      <div className="actions">
        <Button icon={<Plus size={20} />} variant="secondary" block onClick={() => setEditing({})}>Ajouter un frais fixe</Button>
        <div className="presets">
          {PRESETS.map((p) => (
            <Button key={p.label} variant="secondary" size="sm" onClick={() => setEditing(p.draft)}>{p.label}</Button>
          ))}
        </div>
      </div>
      {ended.length > 0 && (
        <>
          <div className="overline" style={{ marginTop: 8 }}>Terminés</div>
          <List>{ended.map(row)}</List>
        </>
      )}
      {editing && (
        <Sheet title={"id" in editing && editing.id ? "Frais fixe" : "Nouveau frais fixe"} onClose={() => setEditing(null)}>
          <FixedCostForm draft={editing} settings={settings} onDone={() => { setEditing(null); reload(); }} />
        </Sheet>
      )}
    </section>
  );
}

function FixedCostForm({ draft, settings, onDone }: { draft: Draft; settings: Settings | null; onDone: () => void }) {
  const leasing = settings?.acquisition_type === "leasing";
  const [kind, setKind] = useState<FixedKind>(draft.kind ?? "vehicle_option");
  const [label, setLabel] = useState(draft.label ?? "");
  const [amount, setAmount] = useState(draft.amount_cents != null ? money(draft.amount_cents).replace(/\s/g, "") : "");
  const [frequency, setFrequency] = useState<"monthly" | "yearly">(draft.frequency ?? "monthly");
  const [start, setStart] = useState(draft.start_date ?? localDate());
  const [end, setEnd] = useState(draft.end_date ?? "");
  const [coverage, setCoverage] = useState<Coverage>(leasing ? "full" : draft.coverage ?? "full");
  const [notes, setNotes] = useState(draft.notes ?? "");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const cur = settings?.currency ?? "CHF";
  const cents = parseMoney(amount);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    if (cents === null || Number.isNaN(cents)) return setError("Montant invalide");
    const input = {
      kind, label, amount_cents: cents, frequency, start_date: start, end_date: end || null,
      coverage: kind === "insurance" ? coverage : null, notes,
    };
    setBusy(true);
    try {
      if (draft.id) await api.updateFixedCost(draft.id, input);
      else await api.createFixedCost(input);
      onDone();
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setBusy(false);
    }
  };
  const remove = async () => {
    if (!draft.id || !confirm("Supprimer ce frais fixe ? Il disparaît aussi des mois passés.")) return;
    setBusy(true);
    try {
      await api.deleteFixedCost(draft.id);
      onDone();
    } catch (err) {
      setError((err as Error).message);
      setBusy(false);
    }
  };

  return (
    <form className="form" onSubmit={submit}>
      <Field label="Type">
        {(id) => (
          <select id={id} value={kind} onChange={(e) => setKind(e.target.value as FixedKind)}>
            {(["insurance", "vehicle_option", "charging_subscription", "tax", "other"] as FixedKind[]).map((k) => (
              <option key={k} value={k}>{FIXED_KINDS[k]}</option>
            ))}
          </select>
        )}
      </Field>
      {kind === "insurance" && (
        <Field label="Couverture" hint={leasing ? "Véhicule en leasing : la casco complète est obligatoire." : undefined}>
          {(id) => (
            <select id={id} value={coverage} onChange={(e) => setCoverage(e.target.value as Coverage)} disabled={leasing}>
              {(Object.keys(COVERAGES) as Coverage[]).map((c) => <option key={c} value={c}>{COVERAGES[c]}</option>)}
            </select>
          )}
        </Field>
      )}
      <Field label="Libellé">
        {(id) => <input id={id} value={label} onChange={(e) => setLabel(e.target.value)} required maxLength={200} placeholder={kind === "insurance" ? "Assurance 2026, compagnie…" : ""} />}
      </Field>
      <Segmented label="Montant saisi" options={[{ value: "monthly", label: "Par mois" }, { value: "yearly", label: "Par an" }]} value={frequency} onChange={setFrequency} />
      <Field
        label={`Montant ${frequency === "yearly" ? "annuel" : "mensuel"} (${cur})`}
        hint={frequency === "yearly" && cents ? `Compté ${cur} ${money(Math.round(cents / 12))} par mois dans les statistiques.` : undefined}
      >
        {(id) => <input id={id} inputMode="decimal" value={amount} onChange={(e) => setAmount(e.target.value)} required />}
      </Field>
      <div className="form-row">
        <Field label="Début">{(id) => <input id={id} type="date" value={start} onChange={(e) => setStart(e.target.value)} required />}</Field>
        <Field label="Fin (optionnel)" hint={kind === "insurance" ? "Une ligne par année de contrat." : "Vide = en cours."}>
          {(id) => <input id={id} type="date" value={end} min={start} onChange={(e) => setEnd(e.target.value)} />}
        </Field>
      </div>
      <Field label="Notes">{(id) => <textarea id={id} value={notes} onChange={(e) => setNotes(e.target.value)} maxLength={2000} />}</Field>
      {error && <div className="form-error" role="alert">{error}</div>}
      <Button type="submit" disabled={busy}>{draft.id ? "Enregistrer" : "Ajouter"}</Button>
      {draft.id && <Button variant="danger" onClick={remove} disabled={busy}>Supprimer</Button>}
    </form>
  );
}
