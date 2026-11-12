-- Connexion au compte Tesla (Fleet API). Une seule ligne.
CREATE TABLE tesla_auth (
    id                 SMALLINT PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    access_token       TEXT NOT NULL,
    refresh_token      TEXT NOT NULL,
    expires_at         TIMESTAMPTZ NOT NULL,
    connected_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_sync_at       TIMESTAMPTZ,
    last_sync_inserted INTEGER,
    last_error         TEXT
);

-- Jetons anti-CSRF du flux OAuth (usage unique, 10 minutes).
CREATE TABLE oauth_states (
    state      TEXT PRIMARY KEY,
    expires_at TIMESTAMPTZ NOT NULL
);
