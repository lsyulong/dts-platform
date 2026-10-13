CREATE UNIQUE INDEX IF NOT EXISTS idx_job_actor_tenant_id ON "user"(tenant_id, id);

CREATE TABLE IF NOT EXISTS product_job (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    source_kind TEXT NOT NULL,
    source_id TEXT NOT NULL,
    source_version INTEGER NOT NULL CHECK (source_version > 0),
    actor_id TEXT NOT NULL,
    action TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    definition_snapshot TEXT NOT NULL CHECK (json_valid(definition_snapshot) AND length(CAST(definition_snapshot AS BLOB)) <= 524288),
    parameters TEXT NOT NULL CHECK (json_valid(parameters) AND length(CAST(parameters AS BLOB)) <= 65536),
    status TEXT NOT NULL CHECK (status IN ('queued', 'running', 'succeeded', 'failed', 'cancelled', 'reconciliation_required')),
    cancel_requested INTEGER NOT NULL DEFAULT 0 CHECK (cancel_requested IN (0, 1)),
    cancel_requested_by TEXT,
    can_retry INTEGER NOT NULL DEFAULT 0 CHECK (can_retry IN (0, 1)),
    attempt INTEGER NOT NULL DEFAULT 0 CHECK (attempt >= 0),
    lease_owner TEXT,
    lease_fence INTEGER NOT NULL DEFAULT 0 CHECK (lease_fence >= 0),
    lease_expires_at TEXT,
    result TEXT CHECK (result IS NULL OR (json_valid(result) AND length(CAST(result AS BLOB)) <= 262144)),
    error TEXT CHECK (error IS NULL OR length(CAST(error AS BLOB)) <= 4000),
    reconciled_by TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    started_at TEXT,
    finished_at TEXT,
    UNIQUE (tenant_id, id),
    UNIQUE (tenant_id, idempotency_key),
    FOREIGN KEY (tenant_id, source_kind, source_id)
        REFERENCES product_resource(tenant_id, kind, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, actor_id) REFERENCES "user"(tenant_id, id) ON DELETE RESTRICT,
    CHECK ((status = 'running' AND lease_owner IS NOT NULL AND lease_expires_at IS NOT NULL AND lease_fence > 0)
        OR (status <> 'running' AND lease_owner IS NULL AND lease_expires_at IS NULL)),
    CHECK ((status IN ('succeeded', 'failed', 'cancelled') AND finished_at IS NOT NULL)
        OR (status NOT IN ('succeeded', 'failed', 'cancelled') AND finished_at IS NULL))
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_product_job_active_source
    ON product_job(tenant_id, source_kind, source_id)
    WHERE status IN ('queued', 'running', 'reconciliation_required');
CREATE INDEX IF NOT EXISTS idx_product_job_claim ON product_job(tenant_id, status, created_at);
CREATE INDEX IF NOT EXISTS idx_product_job_actor ON product_job(tenant_id, actor_id, created_at DESC);

CREATE TABLE IF NOT EXISTS product_job_node (
    tenant_id TEXT NOT NULL,
    job_id TEXT NOT NULL,
    node_id TEXT NOT NULL,
    name TEXT NOT NULL,
    definition TEXT NOT NULL CHECK (json_valid(definition) AND length(CAST(definition AS BLOB)) <= 65536),
    depends_on TEXT NOT NULL CHECK (json_valid(depends_on) AND length(CAST(depends_on AS BLOB)) <= 65536),
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'running', 'succeeded', 'failed', 'cancelled', 'skipped', 'reconciliation_required')),
    attempt INTEGER NOT NULL DEFAULT 0 CHECK (attempt >= 0),
    result TEXT CHECK (result IS NULL OR (json_valid(result) AND length(CAST(result AS BLOB)) <= 262144)),
    error TEXT CHECK (error IS NULL OR length(CAST(error AS BLOB)) <= 4000),
    started_at TEXT,
    finished_at TEXT,
    PRIMARY KEY (tenant_id, job_id, node_id),
    FOREIGN KEY (tenant_id, job_id) REFERENCES product_job(tenant_id, id) ON DELETE CASCADE,
    CHECK ((status IN ('succeeded', 'failed', 'cancelled', 'skipped') AND finished_at IS NOT NULL)
        OR (status IN ('pending', 'running', 'reconciliation_required') AND finished_at IS NULL))
);
