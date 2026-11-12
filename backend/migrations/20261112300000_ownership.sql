-- 1. Leasing : valeur résiduelle, kilométrage inclus, prix du km supplémentaire.
-- 2. Coût de possession : valeur de revente estimée (achat).
-- 5. Recharge à domicile estimée : consommation et pertes.
-- 6. Budget mensuel.
ALTER TABLE settings
    ADD COLUMN leasing_residual_cents       BIGINT CHECK (leasing_residual_cents >= 0),
    ADD COLUMN leasing_km_per_year          INTEGER CHECK (leasing_km_per_year > 0),
    ADD COLUMN leasing_excess_cents_per_km  BIGINT CHECK (leasing_excess_cents_per_km >= 0),
    ADD COLUMN estimated_value_cents        BIGINT CHECK (estimated_value_cents >= 0),
    ADD COLUMN estimate_home_charging       BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN consumption_kwh_per_100km    DOUBLE PRECISION NOT NULL DEFAULT 16 CHECK (consumption_kwh_per_100km > 0),
    ADD COLUMN charging_losses_pct          DOUBLE PRECISION NOT NULL DEFAULT 10 CHECK (charging_losses_pct >= 0 AND charging_losses_pct < 50),
    ADD COLUMN monthly_budget_cents         BIGINT CHECK (monthly_budget_cents >= 0);

-- 3. Taxes et vignette comme frais fixes.
ALTER TABLE fixed_costs DROP CONSTRAINT fixed_costs_kind_check;
ALTER TABLE fixed_costs ADD CONSTRAINT fixed_costs_kind_check
    CHECK (kind IN ('insurance', 'vehicle_option', 'charging_subscription', 'tax', 'other'));

-- 5. Relevés du compteur (Tesla ou saisie manuelle).
CREATE TABLE odometer_readings (
    id       UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    read_at  TIMESTAMPTZ NOT NULL,
    km       INTEGER NOT NULL CHECK (km >= 0),
    source   TEXT NOT NULL CHECK (source IN ('manual', 'tesla_api'))
);
CREATE INDEX odometer_readings_read_at_idx ON odometer_readings (read_at);

-- 4. Rappels d'entretien (date et/ou kilométrage, récurrents ou non).
CREATE TABLE reminders (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title          TEXT NOT NULL,
    due_on         DATE,
    due_km         INTEGER CHECK (due_km >= 0),
    repeat_months  INTEGER CHECK (repeat_months > 0 AND repeat_months <= 120),
    repeat_km      INTEGER CHECK (repeat_km > 0),
    notes          TEXT NOT NULL DEFAULT '',
    last_done_on   DATE,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (due_on IS NOT NULL OR due_km IS NOT NULL)
);
