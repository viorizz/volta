export type Network = "supercharger" | "home" | "public" | "other";
export type Category = "tires" | "maintenance" | "insurance" | "tax" | "accessories" | "parking" | "other";

export interface Charge {
  id: string;
  started_at: string;
  network: Network;
  location: string;
  energy_kwh: number;
  cost_cents: number;
  odometer_km: number | null;
  source: "manual" | "import" | "tesla_api";
  external_id: string | null;
  notes: string;
}
export type ChargeInput = Omit<Charge, "id" | "source" | "external_id"> & { external_id?: string | null };

export interface Expense {
  id: string;
  spent_on: string;
  category: Category;
  label: string;
  amount_cents: number;
  odometer_km: number | null;
  notes: string;
}
export type ExpenseInput = Omit<Expense, "id">;

export interface Settings {
  vehicle_name: string;
  purchase_date: string | null;
  purchase_price_cents: number | null;
  initial_odometer_km: number;
  current_odometer_km: number | null;
  currency: string;
  fuel_consumption_l_per_100km: number;
  fuel_price_cents_per_l: number;
  home_price_cents_per_kwh: number;
}

export interface Group { key: string; cost_cents: number; energy_kwh: number; count: number }
export interface Month { month: string; charge_cost_cents: number; energy_kwh: number; expense_cents: number; fuel_equivalent_cents: number | null }
export interface Stats {
  currency: string;
  from: string | null;
  charge_cost_cents: number;
  energy_kwh: number;
  charge_count: number;
  by_network: Group[];
  expense_cents: number;
  expense_count: number;
  by_category: Group[];
  total_cost_cents: number;
  purchase_price_cents: number | null;
  comparison: {
    distance_km: number | null;
    distance_is_estimate: boolean;
    km_per_kwh: number | null;
    efficiency_plausible: boolean;
    electric_cost_cents: number;
    fuel_cost_cents: number | null;
    savings_cents: number | null;
    electric_per_100km_cents: number | null;
    fuel_per_100km_cents: number;
  };
  monthly: Month[];
}

export interface TeslaStatus {
  configured: boolean;
  connected: boolean;
  redirect_uri: string | null;
  connected_at: string | null;
  last_sync_at: string | null;
  last_sync_inserted: number | null;
  last_error: string | null;
}

export class ApiError extends Error {
  constructor(public status: number, message: string) {
    super(message);
  }
}

/** Appelé quand le serveur répond 401 : l'app revient à l'écran de connexion. */
let onUnauthorized: () => void = () => {};
export function setUnauthorizedHandler(fn: () => void) {
  onUnauthorized = fn;
}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const res = await fetch(`/api${path}`, {
    method,
    credentials: "same-origin",
    headers: body !== undefined ? { "Content-Type": "application/json" } : undefined,
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
  if (res.status === 401 && path !== "/auth/login") onUnauthorized();
  if (!res.ok) {
    const data = await res.json().catch(() => null);
    throw new ApiError(res.status, data?.error ?? `Erreur ${res.status}`);
  }
  if (res.status === 204) return undefined as T;
  return res.json();
}

const qs = (params: Record<string, string | undefined>) => {
  const p = Object.entries(params).filter(([, v]) => v) as [string, string][];
  return p.length ? "?" + new URLSearchParams(p).toString() : "";
};

export const api = {
  login: (password: string) => request<void>("POST", "/auth/login", { password }),
  logout: () => request<void>("POST", "/auth/logout"),
  me: () => request<{ authenticated: boolean }>("GET", "/auth/me"),

  charges: () => request<Charge[]>("GET", "/charges"),
  createCharge: (c: ChargeInput) => request<Charge>("POST", "/charges", c),
  updateCharge: (id: string, c: ChargeInput) => request<Charge>("PUT", `/charges/${id}`, c),
  deleteCharge: (id: string) => request<void>("DELETE", `/charges/${id}`),
  importCharges: (rows: ChargeInput[]) => request<{ inserted: number; skipped: number }>("POST", "/charges/import", { rows }),

  expenses: () => request<Expense[]>("GET", "/expenses"),
  createExpense: (e: ExpenseInput) => request<Expense>("POST", "/expenses", e),
  updateExpense: (id: string, e: ExpenseInput) => request<Expense>("PUT", `/expenses/${id}`, e),
  deleteExpense: (id: string) => request<void>("DELETE", `/expenses/${id}`),

  settings: () => request<Settings>("GET", "/settings"),
  saveSettings: (s: Settings) => request<Settings>("PUT", "/settings", s),

  teslaStatus: () => request<TeslaStatus>("GET", "/tesla/status"),
  teslaConnect: () => request<{ url: string }>("POST", "/tesla/connect"),
  teslaSync: () => request<{ received: number; inserted: number; updated: number }>("POST", "/tesla/sync"),
  teslaDisconnect: () => request<void>("POST", "/tesla/disconnect"),

  stats: (from?: string) => request<Stats>("GET", `/stats${qs({ from })}`),
};
