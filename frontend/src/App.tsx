import { useEffect, useState } from "react";
import { Settings as SettingsIcon, ChevronLeft } from "lucide-react";
import { api, setUnauthorizedHandler } from "./api";
import { Segmented } from "./ui";
import Login from "./pages/Login";
import Overview from "./pages/Overview";
import Charges from "./pages/Charges";
import Expenses from "./pages/Expenses";
import SettingsPage from "./pages/Settings";

type Tab = "apercu" | "recharges" | "depenses" | "reglages";
const TABS: { value: Tab; label: string }[] = [
  { value: "apercu", label: "Aperçu" },
  { value: "recharges", label: "Recharges" },
  { value: "depenses", label: "Dépenses" },
];
const readHash = (): Tab => {
  const h = location.hash.slice(1) as Tab;
  return ["apercu", "recharges", "depenses", "reglages"].includes(h) ? h : "apercu";
};

export default function App() {
  const [auth, setAuth] = useState<"checking" | "in" | "out">("checking");
  const [tab, setTab] = useState<Tab>(readHash);
  const [vehicle, setVehicle] = useState("Volta");

  useEffect(() => {
    setUnauthorizedHandler(() => setAuth("out"));
    api.me().then(() => setAuth("in")).catch(() => setAuth("out"));
    const onHash = () => setTab(readHash());
    window.addEventListener("hashchange", onHash);
    return () => window.removeEventListener("hashchange", onHash);
  }, []);

  useEffect(() => {
    if (auth === "in") api.settings().then((s) => setVehicle(s.vehicle_name)).catch(() => {});
  }, [auth, tab]);

  const go = (t: Tab) => {
    location.hash = t;
    setTab(t);
    window.scrollTo(0, 0);
  };

  if (auth === "checking") return null;
  if (auth === "out") return <Login onDone={() => setAuth("in")} />;

  return (
    <div className="app">
      <header className="topbar">
        {tab === "reglages" ? (
          <button className="iconbtn" onClick={() => go("apercu")} aria-label="Retour">
            <ChevronLeft size={26} strokeWidth={1.5} />
          </button>
        ) : (
          <h1>{vehicle}</h1>
        )}
        {tab === "reglages" ? <h1>Réglages</h1> : null}
        {tab !== "reglages" ? (
          <button className="iconbtn" onClick={() => go("reglages")} aria-label="Réglages">
            <SettingsIcon size={22} strokeWidth={1.5} />
          </button>
        ) : (
          <span style={{ width: 44 }} />
        )}
      </header>

      {tab !== "reglages" && (
        <nav className="tabs">
          <Segmented label="Sections" hideLabel options={TABS} value={tab} onChange={go} />
        </nav>
      )}

      <main>
        {tab === "apercu" && <Overview />}
        {tab === "recharges" && <Charges />}
        {tab === "depenses" && <Expenses />}
        {tab === "reglages" && <SettingsPage onLogout={() => setAuth("out")} />}
      </main>
    </div>
  );
}
