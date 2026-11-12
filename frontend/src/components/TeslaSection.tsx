import { useEffect, useState } from "react";
import { RefreshCw, Zap } from "lucide-react";
import { api, type TeslaStatus } from "../api";
import { Badge, Button, ButtonGroup } from "../ui";

/** Lit le retour de la connexion Tesla (?tesla=connected|error&msg=…) puis nettoie l'URL. */
function readCallback(): { ok: boolean; text: string } | null {
  const q = new URLSearchParams(location.search);
  const t = q.get("tesla");
  if (!t) return null;
  history.replaceState(null, "", location.pathname + location.hash);
  if (t === "connected") return { ok: true, text: "Compte Tesla connecté. Première synchronisation en cours…" };
  let msg = "Connexion refusée";
  try {
    const b64 = (q.get("msg") ?? "").replace(/-/g, "+").replace(/_/g, "/");
    msg = new TextDecoder().decode(Uint8Array.from(atob(b64), (c) => c.charCodeAt(0)));
  } catch {
    /* message illisible : texte par défaut */
  }
  return { ok: false, text: msg };
}

const when = (iso: string) =>
  new Date(iso).toLocaleString("fr-CH", { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });

export default function TeslaSection() {
  const [st, setSt] = useState<TeslaStatus | null>(null);
  const [msg, setMsg] = useState(readCallback);
  const [busy, setBusy] = useState(false);

  const load = () => api.teslaStatus().then(setSt).catch((e) => setMsg({ ok: false, text: e.message }));
  useEffect(() => {
    load();
    // Après une connexion, la première synchro tourne côté serveur : on rafraîchit le statut.
    const t = setTimeout(load, 6000);
    return () => clearTimeout(t);
  }, []);

  const run = async (fn: () => Promise<void>) => {
    setBusy(true);
    setMsg(null);
    try {
      await fn();
    } catch (e) {
      setMsg({ ok: false, text: (e as Error).message });
    } finally {
      setBusy(false);
      load();
    }
  };
  const connect = () => run(async () => {
    const { url } = await api.teslaConnect();
    location.href = url;
  });
  const sync = () => run(async () => {
    const r = await api.teslaSync();
    setMsg({ ok: true, text: `${r.received} session${r.received > 1 ? "s" : ""} reçue${r.received > 1 ? "s" : ""} · ${r.inserted} nouvelle${r.inserted > 1 ? "s" : ""} · ${r.updated} mise${r.updated > 1 ? "s" : ""} à jour` });
  });
  const disconnect = () => {
    if (confirm("Déconnecter le compte Tesla ? Les recharges déjà importées sont conservées.")) run(() => api.teslaDisconnect());
  };

  if (!st) return null;
  return (
    <div className="form">
      <div className="section-head">
        <h2 className="overline" style={{ margin: 0 }}>Compte Tesla</h2>
        {st.connected ? <Badge tone="charge">Connecté</Badge> : <Badge>Non connecté</Badge>}
      </div>

      {!st.configured && (
        <p className="muted" style={{ margin: 0 }}>
          L'intégration n'est pas configurée sur le serveur : ajoute TESLA_CLIENT_ID, TESLA_CLIENT_SECRET et PUBLIC_URL dans /opt/volta/.env (voir le README).
        </p>
      )}

      {st.configured && !st.connected && (
        <>
          <p className="muted" style={{ margin: 0 }}>
            Importe automatiquement tes recharges Supercharger (toutes les 12 heures). Les autres réseaux restent à saisir à la main.
          </p>
          <Button icon={<Zap size={20} />} onClick={connect} disabled={busy}>Connecter mon compte Tesla</Button>
        </>
      )}

      {st.connected && (
        <>
          <div className="muted">
            {st.last_sync_at
              ? `Dernière synchro : ${when(st.last_sync_at)}${st.last_sync_inserted != null ? ` · ${st.last_sync_inserted} nouvelle${st.last_sync_inserted > 1 ? "s" : ""}` : ""}`
              : "Pas encore synchronisé"}
          </div>
          {st.last_error && <div className="form-error">Dernière erreur : {st.last_error}</div>}
          <ButtonGroup>
            <Button variant="outline" icon={<RefreshCw size={24} strokeWidth={1.5} />} onClick={sync} disabled={busy}>
              {busy ? "Synchronisation…" : "Synchroniser maintenant"}
            </Button>
          </ButtonGroup>
          <Button variant="danger" onClick={disconnect} disabled={busy}>Déconnecter Tesla</Button>
        </>
      )}

      {msg && <div className={msg.ok ? "muted" : "form-error"} role="status">{msg.text}</div>}
    </div>
  );
}
