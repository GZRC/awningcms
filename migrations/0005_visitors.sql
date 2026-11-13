-- Public customer identities never share credentials or sessions with staff.
CREATE TABLE cms_visitor_accounts (
    id uuid PRIMARY KEY,
    site_id uuid NOT NULL REFERENCES cms_sites(id) ON DELETE CASCADE,
    email text NOT NULL,
    name text NOT NULL,
    password_hash text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE(site_id, email),
    UNIQUE(site_id, id)
);

CREATE TABLE cms_visitor_sessions (
    token_hash text PRIMARY KEY,
    site_id uuid NOT NULL,
    account_id uuid NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    FOREIGN KEY(site_id, account_id) REFERENCES cms_visitor_accounts(site_id, id) ON DELETE CASCADE
);
CREATE INDEX cms_visitor_sessions_account ON cms_visitor_sessions(site_id, account_id, created_at);
CREATE INDEX cms_visitor_sessions_expiry ON cms_visitor_sessions(expires_at);
