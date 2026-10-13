CREATE UNIQUE INDEX IF NOT EXISTS idx_user_tenant_id ON "user"(tenant_id,id);
CREATE TABLE IF NOT EXISTS development_revision (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    source_kind TEXT NOT NULL CHECK(source_kind IN ('sql_document','development_workflow')),
    source_id TEXT NOT NULL,
    source_version BIGINT NOT NULL CHECK(source_version>0),
    snapshot JSONB NOT NULL CHECK(octet_length(snapshot::text)<=327680),
    snapshot_hash TEXT NOT NULL CHECK(snapshot_hash ~ '^[0-9a-f]{64}$'),
    saved_by TEXT NOT NULL,
    saved_at TIMESTAMPTZ NOT NULL,
    reason TEXT NOT NULL CHECK(length(reason)>0 AND length(reason)<=8192),
    UNIQUE(tenant_id,source_kind,source_id,source_version),
    UNIQUE(tenant_id,id),
    CHECK(snapshot ?& ARRAY['tenant_id','kind','id','version'] AND snapshot->>'tenant_id' IS NOT NULL AND snapshot->>'kind' IS NOT NULL AND snapshot->>'id' IS NOT NULL AND snapshot->>'version' IS NOT NULL AND snapshot->>'tenant_id'=tenant_id AND snapshot->>'kind'=source_kind
      AND snapshot->>'id'=source_id AND (snapshot->>'version')::bigint=source_version),
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
    semantic JSONB NOT NULL,
    result_snapshot JSONB NOT NULL CHECK(octet_length(result_snapshot::text)<=393216),
    created_at TIMESTAMPTZ NOT NULL,
    UNIQUE(tenant_id,idempotency_key),
    FOREIGN KEY(tenant_id,revision_id) REFERENCES development_revision(tenant_id,id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,actor_id) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);
CREATE OR REPLACE FUNCTION guard_development_revision_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'immutable development revision' USING ERRCODE='23514'; END $$;
DROP TRIGGER IF EXISTS development_revision_immutable ON development_revision;
CREATE TRIGGER development_revision_immutable BEFORE UPDATE OR DELETE ON development_revision FOR EACH ROW EXECUTE FUNCTION guard_development_revision_immutable();
DROP TRIGGER IF EXISTS development_revision_receipt_immutable ON development_revision_receipt;
CREATE TRIGGER development_revision_receipt_immutable BEFORE UPDATE OR DELETE ON development_revision_receipt FOR EACH ROW EXECUTE FUNCTION guard_development_revision_immutable();
