-- Existing client credentials remain the primary administrator of their site.
ALTER TABLE cms_extranet_accounts DROP CONSTRAINT cms_extranet_accounts_site_id_key;
ALTER TABLE cms_extranet_accounts
    ADD COLUMN role text NOT NULL DEFAULT 'admin' CHECK (role IN ('admin','editor','viewer')),
    ADD COLUMN active boolean NOT NULL DEFAULT true,
    ADD COLUMN is_primary boolean NOT NULL DEFAULT false;
UPDATE cms_extranet_accounts SET is_primary=true;
CREATE UNIQUE INDEX cms_extranet_accounts_email ON cms_extranet_accounts(site_id,email);
CREATE UNIQUE INDEX cms_extranet_accounts_primary ON cms_extranet_accounts(site_id) WHERE is_primary;

CREATE TABLE cms_extranet_team_invites (
    id uuid PRIMARY KEY,
    site_id uuid NOT NULL REFERENCES cms_sites(id) ON DELETE CASCADE,
    email text NOT NULL,
    name text NOT NULL,
    role text NOT NULL CHECK (role IN ('admin','editor','viewer')),
    token_hash text NOT NULL UNIQUE,
    owner_id uuid REFERENCES cms_users(id) ON DELETE CASCADE,
    account_id uuid REFERENCES cms_extranet_accounts(id) ON DELETE CASCADE,
    expires_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    CHECK (num_nonnulls(owner_id,account_id)=1),
    UNIQUE(site_id,email)
);
ALTER TABLE cms_extranet_audit DROP CONSTRAINT cms_extranet_audit_event_check;
ALTER TABLE cms_extranet_audit
    ADD COLUMN actor_account_id uuid REFERENCES cms_extranet_accounts(id) ON DELETE SET NULL,
    ADD COLUMN target_account_id uuid REFERENCES cms_extranet_accounts(id) ON DELETE SET NULL,
    ADD CONSTRAINT cms_extranet_audit_event_check CHECK (event IN (
        'activation_invited','account_activated','password_reset','support_grant','support_entered',
        'team_invited','team_joined','team_updated','team_invitation_revoked','team_password_reset'
    ));
