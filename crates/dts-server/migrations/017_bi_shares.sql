CREATE TABLE IF NOT EXISTS bi_share (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    report_kind TEXT NOT NULL DEFAULT 'bi_report' CHECK (report_kind = 'bi_report'),
    report_id TEXT NOT NULL,
    report_version BIGINT NOT NULL CHECK (report_version > 0),
    job_id TEXT NOT NULL,
    node_id TEXT NOT NULL,
    owner_id TEXT NOT NULL,
    recipient_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'revoked')),
    version BIGINT NOT NULL DEFAULT 1 CHECK (version > 0),
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    revoked_at TIMESTAMPTZ,
    UNIQUE (tenant_id, id),
    FOREIGN KEY (tenant_id, report_kind, report_id) REFERENCES product_resource(tenant_id, kind, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, job_id, node_id) REFERENCES product_job_node(tenant_id, job_id, node_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, owner_id) REFERENCES "user"(tenant_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, recipient_id) REFERENCES "user"(tenant_id, id) ON DELETE RESTRICT,
    CHECK (owner_id <> recipient_id),
    CHECK (expires_at > created_at AND isfinite(expires_at)),
    CHECK ((status = 'active' AND revoked_at IS NULL) OR (status = 'revoked' AND revoked_at IS NOT NULL))
);
CREATE INDEX IF NOT EXISTS idx_bi_share_owner ON bi_share(tenant_id, owner_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_bi_share_recipient ON bi_share(tenant_id, recipient_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_bi_share_report ON bi_share(tenant_id, report_id, report_version);

CREATE TABLE IF NOT EXISTS bi_share_audit (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    share_id TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    operation TEXT NOT NULL CHECK (operation IN ('create', 'open', 'revoke')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    FOREIGN KEY (tenant_id, share_id) REFERENCES bi_share(tenant_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, actor_id) REFERENCES "user"(tenant_id, id) ON DELETE RESTRICT
);
CREATE INDEX IF NOT EXISTS idx_bi_share_audit ON bi_share_audit(tenant_id, share_id, created_at DESC);
