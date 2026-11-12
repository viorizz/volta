import { useState } from "react";
import { Plus, Upload } from "lucide-react";
import { api, type Charge } from "../api";
import { useLoad } from "../useLoad";
import { NETWORKS, dateShort, km, kwh, money, monthLabel } from "../format";
import { Badge, Button, ButtonGroup, List, ListRow, Sheet } from "../ui";
import ChargeForm from "../components/ChargeForm";
import CsvImport from "../components/CsvImport";

export function groupByMonth<T>(items: T[], date: (t: T) => string) {
  const groups = new Map<string, T[]>();
  for (const it of items) {
    const d = new Date(date(it).length === 10 ? date(it) + "T12:00:00" : date(it));
    const key = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}`;
    groups.set(key, [...(groups.get(key) ?? []), it]);
  }
  return [...groups.entries()];
}

export default function Charges() {
  const { data, error, reload } = useLoad(() => api.charges());
  const { data: settings } = useLoad(() => api.settings());
  const [editing, setEditing] = useState<Charge | "new" | null>(null);
  const [importing, setImporting] = useState(false);
  const cur = settings?.currency ?? "CHF";

  return (
    <>
      <section className="section">
        <div className="actions">
          <Button icon={<Plus size={20} />} onClick={() => setEditing("new")}>Ajouter une recharge</Button>
          <ButtonGroup>
            <Button variant="outline" icon={<Upload size={24} strokeWidth={1.5} />} onClick={() => setImporting(true)}>Importer un CSV</Button>
          </ButtonGroup>
        </div>
      </section>

      {error && <div className="empty">{error}</div>}
      {data && data.length === 0 && <div className="empty">Aucune recharge. Ajoute la première ou importe ton historique.</div>}
      {data && groupByMonth(data, (c) => c.started_at).map(([month, items]) => (
        <section className="section" key={month}>
          <div className="month-head">
            <span className="overline">{monthLabel(month)}</span>
            <span className="overline num">{cur} {money(items.reduce((s, c) => s + c.cost_cents, 0))}</span>
          </div>
          <List>
            {items.map((c) => (
              <ListRow
                key={c.id}
                onClick={() => setEditing(c)}
                title={c.location || NETWORKS[c.network]}
                subtitle={`${dateShort(c.started_at)} · ${kwh(c.energy_kwh)} kWh${c.odometer_km != null ? ` · ${km(c.odometer_km)} km` : ""}`}
                trailing={
                  <>
                    <div>{cur} {money(c.cost_cents)}</div>
                    <Badge tone={c.network === "supercharger" ? "electric" : undefined}>{NETWORKS[c.network]}</Badge>
                  </>
                }
              />
            ))}
          </List>
        </section>
      ))}

      {editing && (
        <Sheet title={editing === "new" ? "Nouvelle recharge" : "Recharge"} onClose={() => setEditing(null)}>
          <ChargeForm charge={editing === "new" ? undefined : editing} settings={settings} onDone={() => { setEditing(null); reload(); }} />
        </Sheet>
      )}
      {importing && (
        <Sheet title="Importer des recharges" onClose={() => setImporting(false)}>
          <CsvImport onDone={reload} />
        </Sheet>
      )}
    </>
  );
}
