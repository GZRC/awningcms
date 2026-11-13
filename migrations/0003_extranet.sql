CREATE TABLE cms_extranet_accounts (
    id uuid PRIMARY KEY,
    site_id uuid NOT NULL UNIQUE REFERENCES cms_sites(id) ON DELETE CASCADE,
    email text NOT NULL,
    name text NOT NULL,
    password_hash text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE cms_extranet_sessions (
    token_hash text PRIMARY KEY,
    site_id uuid NOT NULL REFERENCES cms_sites(id) ON DELETE CASCADE,
    account_id uuid REFERENCES cms_extranet_accounts(id) ON DELETE CASCADE,
    owner_id uuid REFERENCES cms_users(id) ON DELETE CASCADE,
    expires_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    CHECK (num_nonnulls(account_id, owner_id) = 1)
);
CREATE INDEX cms_extranet_sessions_site ON cms_extranet_sessions(site_id, created_at);
CREATE TABLE cms_extranet_invites (
    token_hash text PRIMARY KEY,
    site_id uuid NOT NULL UNIQUE REFERENCES cms_sites(id) ON DELETE CASCADE,
    created_by uuid NOT NULL REFERENCES cms_users(id) ON DELETE CASCADE,
    expires_at timestamptz NOT NULL
);
CREATE TABLE cms_extranet_access_grants (
    token_hash text PRIMARY KEY,
    site_id uuid NOT NULL REFERENCES cms_sites(id) ON DELETE CASCADE,
    owner_id uuid NOT NULL REFERENCES cms_users(id) ON DELETE CASCADE,
    expires_at timestamptz NOT NULL
);
CREATE TABLE cms_extranet_audit (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    site_id uuid NOT NULL REFERENCES cms_sites(id) ON DELETE CASCADE,
    owner_id uuid REFERENCES cms_users(id) ON DELETE SET NULL,
    event text NOT NULL CHECK (event IN ('activation_invited','account_activated','password_reset','support_grant','support_entered')),
    created_at timestamptz NOT NULL DEFAULT now()
);
