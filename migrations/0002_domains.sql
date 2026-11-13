CREATE TABLE cms_domain_bindings (
    domain text PRIMARY KEY,
    site_id uuid NOT NULL UNIQUE REFERENCES cms_sites(id) ON DELETE CASCADE,
    status text NOT NULL CHECK (status IN ('pending_tls', 'connected')),
    verified_at timestamptz NOT NULL,
    connected_at timestamptz,
    updated_at timestamptz NOT NULL DEFAULT now(),
    CHECK (domain = lower(domain)),
    CHECK (status <> 'connected' OR connected_at IS NOT NULL)
);

CREATE TABLE cms_domain_jobs (
    id uuid PRIMARY KEY,
    site_id uuid NOT NULL REFERENCES cms_sites(id) ON DELETE CASCADE,
    domain text NOT NULL,
    status text NOT NULL CHECK (status IN ('queued', 'running', 'waiting_dns', 'connected', 'failed')),
    attempts integer NOT NULL DEFAULT 0 CHECK (attempts BETWEEN 0 AND 4),
    message text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    next_retry_at timestamptz,
    lease_until timestamptz,
    CHECK (domain = lower(domain))
);
CREATE UNIQUE INDEX cms_domain_jobs_active_site ON cms_domain_jobs(site_id)
    WHERE status IN ('queued', 'running', 'waiting_dns');
CREATE UNIQUE INDEX cms_domain_jobs_active_domain ON cms_domain_jobs(domain)
    WHERE status IN ('queued', 'running', 'waiting_dns');
CREATE INDEX cms_domain_jobs_due ON cms_domain_jobs(next_retry_at)
    WHERE status IN ('queued', 'running', 'waiting_dns');
CREATE INDEX cms_domain_jobs_site_history ON cms_domain_jobs(site_id, created_at DESC);
