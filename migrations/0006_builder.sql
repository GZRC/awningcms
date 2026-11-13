ALTER TABLE cms_contents
    ADD COLUMN layout_draft jsonb,
    ADD COLUMN revision bigint NOT NULL DEFAULT 1 CHECK (revision > 0);
CREATE TABLE cms_design_documents (
    site_id uuid NOT NULL REFERENCES cms_sites(id) ON DELETE CASCADE,
    target text NOT NULL CHECK (target IN ('home','template')),
    draft jsonb NOT NULL,
    revision bigint NOT NULL CHECK (revision > 0),
    published_snapshot jsonb,
    published_revision bigint CHECK (published_revision > 0 AND published_revision <= revision),
    published_at timestamptz,
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY(site_id,target),
    CHECK ((published_snapshot IS NULL AND published_revision IS NULL AND published_at IS NULL)
        OR (published_snapshot IS NOT NULL AND published_revision IS NOT NULL AND published_at IS NOT NULL))
);
