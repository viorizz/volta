-- Mode d'acquisition et contrat de leasing.
ALTER TABLE settings
    ADD COLUMN acquisition_type          TEXT NOT NULL DEFAULT 'purchase' CHECK (acquisition_type IN ('purchase', 'leasing')),
    ADD COLUMN leasing_down_payment_cents BIGINT CHECK (leasing_down_payment_cents >= 0),
    ADD COLUMN leasing_monthly_cents      BIGINT CHECK (leasing_monthly_cents >= 0),
    ADD COLUMN leasing_interest_rate      DOUBLE PRECISION CHECK (leasing_interest_rate >= 0 AND leasing_interest_rate < 50),
    ADD COLUMN leasing_start_date         DATE,
    ADD COLUMN leasing_months             INTEGER CHECK (leasing_months > 0 AND leasing_months <= 120);

-- Frais fixes : assurance, options du véhicule, abonnements de recharge…
-- Un montant annuel est réparti sur 12 mois dans les statistiques.
CREATE TABLE fixed_costs (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    kind         TEXT NOT NULL CHECK (kind IN ('insurance', 'vehicle_option', 'charging_subscription', 'other')),
    label        TEXT NOT NULL,
    amount_cents BIGINT NOT NULL CHECK (amount_cents >= 0),
    frequency    TEXT NOT NULL CHECK (frequency IN ('monthly', 'yearly')),
    start_date   DATE NOT NULL,
    end_date     DATE CHECK (end_date IS NULL OR end_date >= start_date),
    coverage     TEXT CHECK (coverage IN ('liability', 'partial', 'full')),
    notes        TEXT NOT NULL DEFAULT '',
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((kind = 'insurance') = (coverage IS NOT NULL))
);
CREATE INDEX fixed_costs_start_idx ON fixed_costs (start_date);
