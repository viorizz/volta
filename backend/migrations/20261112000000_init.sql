-- Réglages du véhicule et du comparatif essence (une seule ligne).
CREATE TABLE settings (
    id                          SMALLINT PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    vehicle_name                TEXT NOT NULL DEFAULT 'Ma voiture',
    purchase_date               DATE,
    purchase_price_cents        BIGINT CHECK (purchase_price_cents >= 0),
    initial_odometer_km         INTEGER NOT NULL DEFAULT 0 CHECK (initial_odometer_km >= 0),
    current_odometer_km         INTEGER CHECK (current_odometer_km >= 0),
    currency                    TEXT NOT NULL DEFAULT 'CHF',
    fuel_consumption_l_per_100km DOUBLE PRECISION NOT NULL DEFAULT 7.0 CHECK (fuel_consumption_l_per_100km > 0),
    fuel_price_cents_per_l      BIGINT NOT NULL DEFAULT 190 CHECK (fuel_price_cents_per_l >= 0),
    home_price_cents_per_kwh    BIGINT NOT NULL DEFAULT 27 CHECK (home_price_cents_per_kwh >= 0),
    updated_at                  TIMESTAMPTZ NOT NULL DEFAULT now()
);
INSERT INTO settings DEFAULT VALUES;

CREATE TABLE charges (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    started_at   TIMESTAMPTZ NOT NULL,
    network      TEXT NOT NULL CHECK (network IN ('supercharger', 'home', 'public', 'other')),
    location     TEXT NOT NULL DEFAULT '',
    energy_kwh   DOUBLE PRECISION NOT NULL CHECK (energy_kwh >= 0),
    cost_cents   BIGINT NOT NULL CHECK (cost_cents >= 0),
    odometer_km  INTEGER CHECK (odometer_km >= 0),
    source       TEXT NOT NULL DEFAULT 'manual' CHECK (source IN ('manual', 'import', 'tesla_api')),
    external_id  TEXT UNIQUE,
    notes        TEXT NOT NULL DEFAULT '',
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX charges_started_at_idx ON charges (started_at DESC);

CREATE TABLE expenses (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    spent_on     DATE NOT NULL,
    category     TEXT NOT NULL CHECK (category IN ('tires', 'maintenance', 'insurance', 'tax', 'accessories', 'parking', 'other')),
    label        TEXT NOT NULL,
    amount_cents BIGINT NOT NULL CHECK (amount_cents >= 0),
    odometer_km  INTEGER CHECK (odometer_km >= 0),
    notes        TEXT NOT NULL DEFAULT '',
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX expenses_spent_on_idx ON expenses (spent_on DESC);

-- Sessions de connexion : seul le hash SHA-256 du jeton est stocké.
CREATE TABLE sessions (
    token_hash  BYTEA PRIMARY KEY,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at  TIMESTAMPTZ NOT NULL
);
