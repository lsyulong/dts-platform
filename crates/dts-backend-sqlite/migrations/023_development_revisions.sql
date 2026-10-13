CREATE UNIQUE INDEX IF NOT EXISTS idx_user_tenant_id ON "user"(tenant_id,id);
CREATE TABLE IF NOT EXISTS development_revision (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    source_kind TEXT NOT NULL CHECK(source_kind IN ('sql_document','development_workflow')),
    source_id TEXT NOT NULL,
    source_version INTEGER NOT NULL CHECK(source_version>0),
    snapshot TEXT NOT NULL CHECK(json_valid(snapshot) AND length(CAST(snapshot AS BLOB))<=327680),
    snapshot_hash TEXT NOT NULL CHECK(length(snapshot_hash)=64 AND snapshot_hash NOT GLOB '*[^0-9a-f]*'),
    saved_by TEXT NOT NULL,
    saved_at TEXT NOT NULL,
    reason TEXT NOT NULL CHECK(length(reason)>0 AND length(reason)<=8192),
    UNIQUE(tenant_id,source_kind,source_id,source_version),
    UNIQUE(tenant_id,id),
    CHECK(json_extract(snapshot,'$.tenant_id') IS tenant_id AND json_extract(snapshot,'$.kind') IS source_kind
      AND json_extract(snapshot,'$.id') IS source_id AND json_extract(snapshot,'$.version') IS source_version),
    FOREIGN KEY(tenant_id,source_kind,source_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,saved_by) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);
CREATE INDEX IF NOT EXISTS idx_development_revision_source ON development_revision(tenant_id,source_kind,source_id,source_version DESC);
CREATE TABLE IF NOT EXISTS development_revision_receipt (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    action TEXT NOT NULL CHECK(action IN ('save','restore')),
    revision_id TEXT NOT NULL,
    semantic TEXT NOT NULL CHECK(json_valid(semantic)),
    result_snapshot TEXT NOT NULL CHECK(json_valid(result_snapshot) AND length(CAST(result_snapshot AS BLOB))<=393216),
    created_at TEXT NOT NULL,
    UNIQUE(tenant_id,idempotency_key),
    FOREIGN KEY(tenant_id,revision_id) REFERENCES development_revision(tenant_id,id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,actor_id) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);
CREATE TRIGGER IF NOT EXISTS development_revision_no_update BEFORE UPDATE ON development_revision BEGIN SELECT RAISE(ABORT,'immutable development revision'); END;
CREATE TRIGGER IF NOT EXISTS development_revision_no_delete BEFORE DELETE ON development_revision BEGIN SELECT RAISE(ABORT,'immutable development revision'); END;
CREATE TRIGGER IF NOT EXISTS development_revision_receipt_no_update BEFORE UPDATE ON development_revision_receipt BEGIN SELECT RAISE(ABORT,'immutable development revision receipt'); END;
CREATE TRIGGER IF NOT EXISTS development_revision_receipt_no_delete BEFORE DELETE ON development_revision_receipt BEGIN SELECT RAISE(ABORT,'immutable development revision receipt'); END;
