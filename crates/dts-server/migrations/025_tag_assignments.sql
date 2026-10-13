CREATE TABLE IF NOT EXISTS tag_assignment (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    tag_kind TEXT NOT NULL DEFAULT 'tag' CHECK(tag_kind='tag'),
    tag_id TEXT NOT NULL,
    tag_version BIGINT NOT NULL CHECK(tag_version>0),
    target_kind TEXT NOT NULL CHECK(length(target_kind) BETWEEN 1 AND 128),
    target_id TEXT NOT NULL,
    assigned_by TEXT NOT NULL,
    assigned_at TIMESTAMPTZ NOT NULL,
    UNIQUE(tenant_id,tag_id,target_kind,target_id),
    FOREIGN KEY(tenant_id,tag_kind,tag_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,target_kind,target_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,assigned_by) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);
CREATE INDEX IF NOT EXISTS idx_tag_assignment_tag ON tag_assignment(tenant_id,tag_id,assigned_at DESC,id);
CREATE INDEX IF NOT EXISTS idx_tag_assignment_target ON tag_assignment(tenant_id,target_kind,target_id);

CREATE TABLE IF NOT EXISTS tag_assignment_receipt (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    request_hash TEXT NOT NULL CHECK(length(request_hash)=64),
    action TEXT NOT NULL CHECK(action IN ('assign','unassign')),
    assignment_id TEXT NOT NULL,
    tag_id TEXT NOT NULL,
    tag_version BIGINT NOT NULL CHECK(tag_version>0),
    target_kind TEXT NOT NULL,
    target_id TEXT NOT NULL,
    target_code TEXT NOT NULL,
    target_name TEXT NOT NULL,
    assigned_by TEXT NOT NULL,
    assigned_at TIMESTAMPTZ NOT NULL,
    active BOOLEAN NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    UNIQUE(tenant_id,idempotency_key),
    FOREIGN KEY(tenant_id,actor_id) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);
CREATE OR REPLACE FUNCTION reject_tag_assignment_receipt_mutation() RETURNS TRIGGER AS $$
BEGIN RAISE EXCEPTION 'immutable tag assignment receipt'; END;
$$ LANGUAGE plpgsql;
DROP TRIGGER IF EXISTS tag_assignment_receipt_immutable ON tag_assignment_receipt;
CREATE TRIGGER tag_assignment_receipt_immutable BEFORE UPDATE OR DELETE ON tag_assignment_receipt FOR EACH ROW EXECUTE FUNCTION reject_tag_assignment_receipt_mutation();
