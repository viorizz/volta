import { useState, type FormEvent } from "react";
import { api } from "../api";
import { Button, Field } from "../ui";

export default function Login({ onDone }: { onDone: () => void }) {
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await api.login(password);
      onDone();
    } catch (err) {
      setError((err as Error).message === "Non connecté" ? "Mot de passe incorrect" : (err as Error).message);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="login">
      <form className="login-box" onSubmit={submit}>
        <div>
          <h1>Volta</h1>
          <p className="muted">Chaque kWh, chaque franc, depuis le premier jour.</p>
        </div>
        <Field label="Mot de passe">
          {(id) => <input id={id} type="password" autoComplete="current-password" value={password} onChange={(e) => setPassword(e.target.value)} required />}
        </Field>
        {error && <div className="form-error" role="alert">{error}</div>}
        <Button type="submit" disabled={busy || !password}>Se connecter</Button>
      </form>
    </div>
  );
}
