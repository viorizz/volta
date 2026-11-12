import { useState, type FormEvent } from "react";
import { Check, Plus } from "lucide-react";
import { api, type Reminder } from "../api";
import { useLoad } from "../useLoad";
import { dateShort, km, localDate, parseDecimal } from "../format";
import { Badge, Button, Field, List, ListRow, Sheet } from "../ui";

type Draft = Partial<Reminder>;

/** Prochaine occurrence d'un jour fixe de l'année (ex. 15 octobre). */
function nextDate(month: number, day: number) {
  const now = new Date();
  let y = now.getFullYear();
  if (new Date(y, month - 1, day) < new Date(now.getFullYear(), now.getMonth(), now.getDate())) y++;
  return localDate(new Date(y, month - 1, day));
}

const PRESETS: { label: string; draft: () => Draft }[] = [
  { label: "Pneus d'hiver", draft: () => ({ title: "Monter les pneus d'hiver", due_on: nextDate(10, 15), repeat_months: 12 }) },
  { label: "Pneus d'été", draft: () => ({ title: "Monter les pneus d'été", due_on: nextDate(4, 1), repeat_months: 12 }) },
  { label: "Expertise", draft: () => ({ title: "Expertise (contrôle technique)" }) },
  { label: "Liquide de frein", draft: () => ({ title: "Contrôle du liquide de frein", repeat_months: 24 }) },
  { label: "Filtre habitacle", draft: () => ({ title: "Filtre d'habitacle", repeat_months: 24 }) },
  { label: "Vignette", draft: () => ({ title: "Acheter la vignette", due_on: nextDate(1, 31), repeat_months: 12 }) },
];

export function dueText(r: Reminder) {
  const parts: string[] = [];
  if (r.days_left !== null) {
    const d = r.days_left;
    parts.push(d < 0 ? `en retard de ${-d} j` : d === 0 ? "aujourd'hui" : d <= 60 ? `dans ${d} j` : dateShort(r.due_on!));
  }
  if (r.due_km !== null) {
    parts.push(r.km_left === null ? `à ${km(r.due_km)} km` : r.km_left < 0 ? `dépassé de ${km(-r.km_left)} km` : `dans ${km(r.km_left)} km`);
  }
  return parts.join(" · ");
}

export function StatusBadge({ status }: { status: Reminder["status"] }) {
  if (status === "overdue") return <Badge tone="fuel">En retard</Badge>;
  if (status === "soon") return <Badge tone="electric">Bientôt</Badge>;
  return <Badge>Prévu</Badge>;
}

export default function Maintenance() {
  const { data, error, reload } = useLoad(() => api.reminders());
  const [editing, setEditing] = useState<Draft | null>(null);

  const done = async (r: Reminder) => {
    const msg = r.repeat_months || r.repeat_km ? "Marquer comme fait ? Le rappel sera reprogrammé." : "Marquer comme fait ? Ce rappel unique sera supprimé.";
    if (!confirm(msg)) return;
    try {
      await api.reminderDone(r.id);
      reload();
    } catch (e) {
      alert((e as Error).message);
    }
  };

  return (
    <>
      <OdometerForm />
      <section className="section">
        <div className="section-head"><h2>Rappels</h2></div>
        {error && <div className="empty">{error}</div>}
        {data && data.length === 0 && <div className="empty">Aucun rappel. Pneus, expertise, liquide de frein : ajoute-les pour ne rien oublier.</div>}
        {data && data.length > 0 && (
          <List>
            {data.map((r) => (
              <ListRow
                key={r.id}
                onClick={() => setEditing(r)}
                title={r.title}
                subtitle={dueText(r) + (r.repeat_months ? ` · tous les ${r.repeat_months} mois` : "") + (r.repeat_km ? ` · tous les ${km(r.repeat_km)} km` : "")}
                trailing={
                  <div className="reminder-trail">
                    <StatusBadge status={r.status} />
                    <button className="iconbtn" aria-label={`Marquer « ${r.title} » comme fait`} onClick={(e) => { e.stopPropagation(); done(r); }}>
                      <Check size={22} strokeWidth={1.5} />
                    </button>
                  </div>
                }
              />
            ))}
          </List>
        )}
        <div className="actions">
          <Button variant="secondary" block icon={<Plus size={20} />} onClick={() => setEditing({})}>Ajouter un rappel</Button>
          <div className="presets">
            {PRESETS.map((p) => <Button key={p.label} variant="secondary" size="sm" onClick={() => setEditing(p.draft())}>{p.label}</Button>)}
          </div>
        </div>
      </section>
      {editing && (
        <Sheet title={editing.id ? "Rappel" : "Nouveau rappel"} onClose={() => setEditing(null)}>
          <ReminderForm draft={editing} onDone={() => { setEditing(null); reload(); }} />
        </Sheet>
      )}
    </>
  );
}

function OdometerForm() {
  const [value, setValue] = useState("");
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    const v = parseDecimal(value);
    if (v === null || Number.isNaN(v) || v < 0) return setMsg({ ok: false, text: "Kilométrage invalide" });
    try {
      await api.addOdometer(Math.round(v));
      setValue("");
      setMsg({ ok: true, text: `Relevé de ${km(Math.round(v))} km enregistré.` });
    } catch (err) {
      setMsg({ ok: false, text: (err as Error).message });
    }
  };
  return (
    <section className="section">
      <form className="form" onSubmit={submit} style={{ maxWidth: 420 }}>
        <Field label="Kilométrage actuel" hint="Sert aux rappels en km, au leasing et au comparatif. Lu automatiquement sur la voiture si Tesla est connecté et qu'elle est éveillée.">
          {(id) => <input id={id} inputMode="numeric" value={value} onChange={(e) => setValue(e.target.value)} placeholder="Ex. 16 250" />}
        </Field>
        <Button type="submit" variant="secondary" block disabled={!value.trim()}>Enregistrer le relevé</Button>
        {msg && <div className={msg.ok ? "muted" : "form-error"} role="status">{msg.text}</div>}
      </form>
    </section>
  );
}

function ReminderForm({ draft, onDone }: { draft: Draft; onDone: () => void }) {
  const [title, setTitle] = useState(draft.title ?? "");
  const [dueOn, setDueOn] = useState(draft.due_on ?? "");
  const [dueKm, setDueKm] = useState(draft.due_km != null ? String(draft.due_km) : "");
  const [repeatMonths, setRepeatMonths] = useState(draft.repeat_months != null ? String(draft.repeat_months) : "");
  const [repeatKm, setRepeatKm] = useState(draft.repeat_km != null ? String(draft.repeat_km) : "");
  const [notes, setNotes] = useState(draft.notes ?? "");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const int = (v: string) => {
    if (!v.trim()) return null;
    const n = parseDecimal(v);
    if (n === null || Number.isNaN(n)) throw new Error("Nombre invalide");
    return Math.round(n);
  };
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    try {
      const input = { title, due_on: dueOn || null, due_km: int(dueKm), repeat_months: int(repeatMonths), repeat_km: int(repeatKm), notes };
      setBusy(true);
      if (draft.id) await api.updateReminder(draft.id, input);
      else await api.createReminder(input);
      onDone();
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setBusy(false);
    }
  };
  const remove = async () => {
    if (!draft.id || !confirm("Supprimer ce rappel ?")) return;
    await api.deleteReminder(draft.id).catch((e) => setError(e.message));
    onDone();
  };

  return (
    <form className="form" onSubmit={submit}>
      <Field label="Titre">{(id) => <input id={id} value={title} onChange={(e) => setTitle(e.target.value)} required maxLength={200} />}</Field>
      <div className="form-row">
        <Field label="Échéance (date)">{(id) => <input id={id} type="date" value={dueOn} onChange={(e) => setDueOn(e.target.value)} />}</Field>
        <Field label="ou à (km)">{(id) => <input id={id} inputMode="numeric" value={dueKm} onChange={(e) => setDueKm(e.target.value)} />}</Field>
      </div>
      <div className="form-row">
        <Field label="Répéter tous les (mois)">{(id) => <input id={id} inputMode="numeric" value={repeatMonths} onChange={(e) => setRepeatMonths(e.target.value)} />}</Field>
        <Field label="ou tous les (km)">{(id) => <input id={id} inputMode="numeric" value={repeatKm} onChange={(e) => setRepeatKm(e.target.value)} />}</Field>
      </div>
      <div className="muted" style={{ fontSize: 13 }}>Une date, un kilométrage ou les deux : la première échéance atteinte compte. Sans répétition, le rappel disparaît une fois fait.</div>
      <Field label="Notes">{(id) => <textarea id={id} value={notes} onChange={(e) => setNotes(e.target.value)} maxLength={2000} />}</Field>
      {error && <div className="form-error" role="alert">{error}</div>}
      <Button type="submit" disabled={busy}>{draft.id ? "Enregistrer" : "Ajouter le rappel"}</Button>
      {draft.id && <Button variant="danger" onClick={remove}>Supprimer le rappel</Button>}
    </form>
  );
}
