CREATE TABLE IF NOT EXISTS product_resource (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenant(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    code TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    parent_id TEXT,
    status TEXT NOT NULL,
    properties JSONB NOT NULL,
    version BIGINT NOT NULL DEFAULT 1 CHECK (version > 0),
    created_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE (tenant_id, kind, id),
    UNIQUE (tenant_id, kind, code),
    FOREIGN KEY (tenant_id, kind, parent_id)
        REFERENCES product_resource(tenant_id, kind, id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_product_resource_list
    ON product_resource(tenant_id, kind, status, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_product_resource_parent
    ON product_resource(tenant_id, kind, parent_id);

CREATE TABLE IF NOT EXISTS product_resource_links (
    tenant_id TEXT NOT NULL,
    source_kind TEXT NOT NULL,
    source_id TEXT NOT NULL,
    field TEXT NOT NULL,
    target_kind TEXT NOT NULL,
    target_id TEXT NOT NULL,
    PRIMARY KEY (tenant_id, source_kind, source_id, field, target_kind, target_id),
    FOREIGN KEY (tenant_id, source_kind, source_id)
        REFERENCES product_resource(tenant_id, kind, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, target_kind, target_id)
        REFERENCES product_resource(tenant_id, kind, id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_product_resource_link_target
    ON product_resource_links(tenant_id, target_kind, target_id);

CREATE TABLE IF NOT EXISTS product_resource_audit (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenant(id) ON DELETE CASCADE,
    actor_id TEXT NOT NULL,
    resource_kind TEXT NOT NULL,
    resource_id TEXT NOT NULL,
    operation TEXT NOT NULL CHECK (operation IN ('create', 'update', 'delete')),
    summary JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_product_resource_audit_list
    ON product_resource_audit(tenant_id, resource_kind, resource_id, created_at DESC);
