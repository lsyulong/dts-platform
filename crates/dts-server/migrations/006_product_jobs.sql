CREATE UNIQUE INDEX IF NOT EXISTS idx_job_actor_tenant_id ON "user"(tenant_id, id);

CREATE TABLE IF NOT EXISTS product_job (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    source_kind TEXT NOT NULL,
    source_id TEXT NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    actor_id TEXT NOT NULL,
    action TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    definition_snapshot JSONB NOT NULL CHECK (octet_length(definition_snapshot::text) <= 524288),
    parameters JSONB NOT NULL CHECK (octet_length(parameters::text) <= 65536),
    status TEXT NOT NULL CHECK (status IN ('queued', 'running', 'succeeded', 'failed', 'cancelled', 'reconciliation_required')),
    cancel_requested BOOLEAN NOT NULL DEFAULT FALSE,
    cancel_requested_by TEXT,
    can_retry BOOLEAN NOT NULL DEFAULT FALSE,
    attempt BIGINT NOT NULL DEFAULT 0 CHECK (attempt >= 0),
    lease_owner TEXT,
    lease_fence BIGINT NOT NULL DEFAULT 0 CHECK (lease_fence >= 0),
    lease_expires_at TIMESTAMPTZ,
    result JSONB CHECK (result IS NULL OR octet_length(result::text) <= 262144),
    error TEXT CHECK (error IS NULL OR octet_length(error) <= 4000),
    reconciled_by TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
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
    definition JSONB NOT NULL CHECK (octet_length(definition::text) <= 65536),
    depends_on JSONB NOT NULL CHECK (octet_length(depends_on::text) <= 65536),
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'running', 'succeeded', 'failed', 'cancelled', 'skipped', 'reconciliation_required')),
    attempt BIGINT NOT NULL DEFAULT 0 CHECK (attempt >= 0),
    result JSONB CHECK (result IS NULL OR octet_length(result::text) <= 262144),
    error TEXT CHECK (error IS NULL OR octet_length(error) <= 4000),
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    PRIMARY KEY (tenant_id, job_id, node_id),
    FOREIGN KEY (tenant_id, job_id) REFERENCES product_job(tenant_id, id) ON DELETE CASCADE,
    CHECK ((status IN ('succeeded', 'failed', 'cancelled', 'skipped') AND finished_at IS NOT NULL)
        OR (status IN ('pending', 'running', 'reconciliation_required') AND finished_at IS NULL))
);
