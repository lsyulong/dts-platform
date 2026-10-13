CREATE UNIQUE INDEX IF NOT EXISTS idx_data_access_actor_scope ON "user"(tenant_id,id);

CREATE TABLE IF NOT EXISTS data_access_request (
    tenant_id TEXT NOT NULL,
    request_id TEXT NOT NULL,
    request_kind TEXT NOT NULL DEFAULT 'data_access_request' CHECK (request_kind='data_access_request'),
    requester_id TEXT NOT NULL,
    metadata_id TEXT NOT NULL,
    target_snapshot TEXT NOT NULL CHECK (jsonb_typeof(target_snapshot::jsonb)='object'),
    requested_columns TEXT NOT NULL CHECK (jsonb_typeof(requested_columns::jsonb)='array'),
    purpose TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id,request_id),
    UNIQUE (tenant_id,request_id,requester_id,metadata_id),
    FOREIGN KEY (tenant_id,request_kind,request_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id,requester_id) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id,metadata_id) REFERENCES metadata_catalog(tenant_id,object_id) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS data_access_grant (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    request_id TEXT NOT NULL,
    request_version BIGINT NOT NULL CHECK (request_version>0),
    grantee_id TEXT NOT NULL,
    metadata_id TEXT NOT NULL,
    target_snapshot TEXT NOT NULL CHECK (jsonb_typeof(target_snapshot::jsonb)='object'),
    columns_snapshot TEXT NOT NULL CHECK (jsonb_typeof(columns_snapshot::jsonb)='array'),
    granted_by TEXT NOT NULL,
    granted_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    revoked_at TEXT,
    revoked_by TEXT,
    UNIQUE (tenant_id,id),
    UNIQUE (tenant_id,request_id),
    CHECK ((revoked_at IS NULL)=(revoked_by IS NULL)),
    FOREIGN KEY (tenant_id,request_id,grantee_id,metadata_id) REFERENCES data_access_request(tenant_id,request_id,requester_id,metadata_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id,grantee_id) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id,granted_by) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id,revoked_by) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);
CREATE INDEX IF NOT EXISTS idx_data_access_grant_actor ON data_access_grant(tenant_id,grantee_id,revoked_at,expires_at);

CREATE TABLE IF NOT EXISTS data_access_decision (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    request_id TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('submit','approve','reject','withdraw','revoke')),
    request_semantics TEXT NOT NULL CHECK (jsonb_typeof(request_semantics::jsonb)='object'),
    result_snapshot TEXT NOT NULL CHECK (jsonb_typeof(result_snapshot::jsonb)='object'),
    created_at TEXT NOT NULL,
    UNIQUE (tenant_id,idempotency_key),
    FOREIGN KEY (tenant_id,request_id) REFERENCES data_access_request(tenant_id,request_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id,actor_id) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS data_access_audit (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('query','submit','approve','reject','withdraw','revoke')),
    outcome TEXT NOT NULL CHECK (outcome IN ('succeeded','failed','denied')),
    evidence TEXT NOT NULL CHECK (jsonb_typeof(evidence::jsonb)='object'),
    created_at TEXT NOT NULL,
    FOREIGN KEY (tenant_id,actor_id) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);
CREATE INDEX IF NOT EXISTS idx_data_access_audit_actor ON data_access_audit(tenant_id,actor_id,created_at DESC,id);

CREATE OR REPLACE FUNCTION data_access_immutable() RETURNS trigger AS $$
BEGIN
    IF TG_TABLE_NAME='data_access_grant' AND TG_OP='UPDATE' THEN
        IF (NEW.id,NEW.tenant_id,NEW.request_id,NEW.request_version,NEW.grantee_id,NEW.metadata_id,
            NEW.target_snapshot,NEW.columns_snapshot,NEW.granted_by,NEW.granted_at,NEW.expires_at)
          IS DISTINCT FROM (OLD.id,OLD.tenant_id,OLD.request_id,OLD.request_version,OLD.grantee_id,OLD.metadata_id,
            OLD.target_snapshot,OLD.columns_snapshot,OLD.granted_by,OLD.granted_at,OLD.expires_at)
          OR OLD.revoked_at IS NOT NULL OR NEW.revoked_at IS NULL OR NEW.revoked_by IS NULL THEN
            RAISE EXCEPTION 'data_access_grant_immutable' USING ERRCODE='23514';
        END IF;
        RETURN NEW;
    END IF;
    RAISE EXCEPTION 'data_access_history_immutable' USING ERRCODE='23514';
END;
$$ LANGUAGE plpgsql;
CREATE TRIGGER data_access_receipt_immutable BEFORE UPDATE OR DELETE ON data_access_decision
FOR EACH ROW EXECUTE FUNCTION data_access_immutable();
CREATE TRIGGER data_access_audit_immutable BEFORE UPDATE OR DELETE ON data_access_audit
FOR EACH ROW EXECUTE FUNCTION data_access_immutable();
CREATE TRIGGER data_access_grant_immutable BEFORE UPDATE OR DELETE ON data_access_grant
FOR EACH ROW EXECUTE FUNCTION data_access_immutable();
