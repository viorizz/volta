import { useState, type FormEvent } from "react";
import { Plus } from "lucide-react";
import { api, type Category, type Expense } from "../api";
import { useLoad } from "../useLoad";
import { CATEGORIES, dateShort, km, localDate, money, monthLabel, parseDecimal, parseMoney } from "../format";
import { Badge, Button, Field, List, ListRow, Sheet } from "../ui";
import { groupByMonth } from "./Charges";

export default function Expenses() {
  const { data, error, reload } = useLoad(() => api.expenses());
  const { data: settings } = useLoad(() => api.settings());
  const [editing, setEditing] = useState<Expense | "new" | null>(null);
  const cur = settings?.currency ?? "CHF";

  return (
    <>
      <section className="section">
        <div className="actions">
          <Button icon={<Plus size={20} />} onClick={() => setEditing("new")}>Ajouter une dépense</Button>
        </div>
      </section>
      {error && <div className="empty">{error}</div>}
      {data && data.length === 0 && <div className="empty">Aucune dépense. Pneus, entretien, assurance, vignette… tout va ici.</div>}
      {data && groupByMonth(data, (e) => e.spent_on).map(([month, items]) => (
        <section className="section" key={month}>
          <div className="month-head">
            <span className="overline">{monthLabel(month)}</span>
            <span className="overline num">{cur} {money(items.reduce((s, e) => s + e.amount_cents, 0))}</span>
          </div>
          <List>
            {items.map((e) => (
              <ListRow
                key={e.id}
                onClick={() => setEditing(e)}
                title={e.label}
                subtitle={`${dateShort(e.spent_on)}${e.odometer_km != null ? ` · ${km(e.odometer_km)} km` : ""}`}
                trailing={<><div>{cur} {money(e.amount_cents)}</div><Badge>{CATEGORIES[e.category]}</Badge></>}
              />
            ))}
          </List>
        </section>
      ))}
      {editing && (
        <Sheet title={editing === "new" ? "Nouvelle dépense" : "Dépense"} onClose={() => setEditing(null)}>
          <ExpenseForm expense={editing === "new" ? undefined : editing} currency={cur} onDone={() => { setEditing(null); reload(); }} />
        </Sheet>
      )}
    </>
  );
}

function ExpenseForm({ expense, currency, onDone }: { expense?: Expense; currency: string; onDone: () => void }) {
  const [spentOn, setSpentOn] = useState(expense?.spent_on ?? localDate());
  const [category, setCategory] = useState<Category>(expense?.category ?? "maintenance");
  const [label, setLabel] = useState(expense?.label ?? "");
  const [amount, setAmount] = useState(expense ? money(expense.amount_cents).replace(/\s/g, "") : "");
  const [odometer, setOdometer] = useState(expense?.odometer_km != null ? String(expense.odometer_km) : "");
  const [notes, setNotes] = useState(expense?.notes ?? "");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const submit = async (ev: FormEvent) => {
    ev.preventDefault();
    setError(null);
    const cents = parseMoney(amount);
    const odo = odometer.trim() ? parseDecimal(odometer) : null;
    if (cents === null || Number.isNaN(cents)) return setError("Montant invalide");
    if (odo !== null && Number.isNaN(odo)) return setError("Kilométrage invalide");
    const input = { spent_on: spentOn, category, label, amount_cents: cents, odometer_km: odo === null ? null : Math.round(odo), notes };
    setBusy(true);
    try {
      if (expense) await api.updateExpense(expense.id, input);
      else await api.createExpense(input);
      onDone();
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setBusy(false);
    }
  };
  const remove = async () => {
    if (!expense || !confirm("Supprimer cette dépense ?")) return;
    setBusy(true);
    try {
      await api.deleteExpense(expense.id);
      onDone();
    } catch (err) {
      setError((err as Error).message);
      setBusy(false);
    }
  };

  return (
    <form className="form" onSubmit={submit}>
      <Field label="Catégorie">
        {(id) => (
          <select id={id} value={category} onChange={(e) => setCategory(e.target.value as Category)}>
            {Object.entries(CATEGORIES).map(([v, l]) => <option key={v} value={v}>{l}</option>)}
          </select>
        )}
      </Field>
      <Field label="Libellé">
        {(id) => <input id={id} value={label} onChange={(e) => setLabel(e.target.value)} placeholder="Pneus hiver ×4, service annuel…" required maxLength={200} />}
      </Field>
      <div className="form-row">
        <Field label="Date">{(id) => <input id={id} type="date" value={spentOn} onChange={(e) => setSpentOn(e.target.value)} required />}</Field>
        <Field label={`Montant (${currency})`}>{(id) => <input id={id} inputMode="decimal" value={amount} onChange={(e) => setAmount(e.target.value)} required />}</Field>
      </div>
      <Field label="Kilométrage (optionnel)">{(id) => <input id={id} inputMode="numeric" value={odometer} onChange={(e) => setOdometer(e.target.value)} />}</Field>
      <Field label="Notes">{(id) => <textarea id={id} value={notes} onChange={(e) => setNotes(e.target.value)} maxLength={2000} />}</Field>
      {error && <div className="form-error" role="alert">{error}</div>}
      <Button type="submit" disabled={busy}>{expense ? "Enregistrer" : "Ajouter la dépense"}</Button>
      {expense && <Button variant="danger" onClick={remove} disabled={busy}>Supprimer la dépense</Button>}
    </form>
  );
}
