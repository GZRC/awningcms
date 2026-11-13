CREATE TABLE cms_users (
    id uuid PRIMARY KEY,
    email text NOT NULL UNIQUE,
    name text NOT NULL,
    password_hash text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE cms_setup (
    singleton boolean PRIMARY KEY DEFAULT true CHECK (singleton),
    completed boolean NOT NULL DEFAULT false
);
INSERT INTO cms_setup(singleton) VALUES (true);
CREATE TABLE cms_sessions (
    token_hash text PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES cms_users(id) ON DELETE CASCADE,
    expires_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX cms_sessions_expiry ON cms_sessions(expires_at);
CREATE TABLE cms_rate_limits (
    key text PRIMARY KEY,
    attempts integer NOT NULL,
    expires_at timestamptz NOT NULL
);
CREATE TABLE cms_sites (
    id uuid PRIMARY KEY,
    name text NOT NULL,
    slug text NOT NULL UNIQUE,
    domain text,
    is_primary boolean NOT NULL DEFAULT false,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX cms_sites_one_primary ON cms_sites(is_primary) WHERE is_primary;
CREATE TABLE cms_contents (
    id uuid PRIMARY KEY,
    site_id uuid NOT NULL REFERENCES cms_sites(id) ON DELETE CASCADE,
    kind text NOT NULL CHECK (kind IN ('page', 'article')),
    title text NOT NULL,
    slug text NOT NULL,
    excerpt text NOT NULL DEFAULT '',
    body text NOT NULL DEFAULT '',
    published_snapshot jsonb,
    published_slug text,
    published_at timestamptz,
    updated_at timestamptz NOT NULL DEFAULT now(),
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT published_fields CHECK ((published_snapshot IS NULL AND published_slug IS NULL AND published_at IS NULL) OR (published_snapshot IS NOT NULL AND published_slug IS NOT NULL AND published_at IS NOT NULL)),
    UNIQUE(site_id, slug),
    UNIQUE(site_id, published_slug)
);
CREATE TABLE cms_settings (
    site_id uuid PRIMARY KEY REFERENCES cms_sites(id) ON DELETE CASCADE,
    site_name text NOT NULL DEFAULT 'Cassagneres',
    description text NOT NULL DEFAULT '',
    blog_enabled boolean NOT NULL DEFAULT false
);
