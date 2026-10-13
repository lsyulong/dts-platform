CREATE UNIQUE INDEX IF NOT EXISTS idx_data_access_actor_scope ON "user"(tenant_id,id);

CREATE TABLE IF NOT EXISTS data_access_request (
    tenant_id TEXT NOT NULL,
    request_id TEXT NOT NULL,
    request_kind TEXT NOT NULL DEFAULT 'data_access_request' CHECK (request_kind='data_access_request'),
    requester_id TEXT NOT NULL,
    metadata_id TEXT NOT NULL,
    target_snapshot TEXT NOT NULL CHECK (json_valid(target_snapshot) AND json_type(target_snapshot)='object'),
    requested_columns TEXT NOT NULL CHECK (json_valid(requested_columns) AND json_type(requested_columns)='array'),
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
    request_version INTEGER NOT NULL CHECK (request_version>0),
    grantee_id TEXT NOT NULL,
    metadata_id TEXT NOT NULL,
    target_snapshot TEXT NOT NULL CHECK (json_valid(target_snapshot) AND json_type(target_snapshot)='object'),
    columns_snapshot TEXT NOT NULL CHECK (json_valid(columns_snapshot) AND json_type(columns_snapshot)='array'),
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
    request_semantics TEXT NOT NULL CHECK (json_valid(request_semantics) AND json_type(request_semantics)='object'),
    result_snapshot TEXT NOT NULL CHECK (json_valid(result_snapshot) AND json_type(result_snapshot)='object'),
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
    evidence TEXT NOT NULL CHECK (json_valid(evidence) AND json_type(evidence)='object'),
    created_at TEXT NOT NULL,
    FOREIGN KEY (tenant_id,actor_id) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);
CREATE INDEX IF NOT EXISTS idx_data_access_audit_actor ON data_access_audit(tenant_id,actor_id,created_at DESC,id);

CREATE TRIGGER IF NOT EXISTS data_access_receipt_no_update BEFORE UPDATE ON data_access_decision
BEGIN SELECT RAISE(ABORT,'data_access_receipt_immutable'); END;
CREATE TRIGGER IF NOT EXISTS data_access_receipt_no_delete BEFORE DELETE ON data_access_decision
BEGIN SELECT RAISE(ABORT,'data_access_receipt_immutable'); END;
CREATE TRIGGER IF NOT EXISTS data_access_audit_no_update BEFORE UPDATE ON data_access_audit
BEGIN SELECT RAISE(ABORT,'data_access_audit_immutable'); END;
CREATE TRIGGER IF NOT EXISTS data_access_audit_no_delete BEFORE DELETE ON data_access_audit
BEGIN SELECT RAISE(ABORT,'data_access_audit_immutable'); END;
CREATE TRIGGER IF NOT EXISTS data_access_grant_immutable BEFORE UPDATE ON data_access_grant
WHEN NEW.id<>OLD.id OR NEW.tenant_id<>OLD.tenant_id OR NEW.request_id<>OLD.request_id OR NEW.request_version<>OLD.request_version
 OR NEW.grantee_id<>OLD.grantee_id OR NEW.metadata_id<>OLD.metadata_id OR NEW.target_snapshot<>OLD.target_snapshot
 OR NEW.columns_snapshot<>OLD.columns_snapshot OR NEW.granted_by<>OLD.granted_by OR NEW.granted_at<>OLD.granted_at
 OR NEW.expires_at<>OLD.expires_at OR OLD.revoked_at IS NOT NULL OR NEW.revoked_at IS NULL OR NEW.revoked_by IS NULL
BEGIN SELECT RAISE(ABORT,'data_access_grant_immutable'); END;
CREATE TRIGGER IF NOT EXISTS data_access_grant_no_delete BEFORE DELETE ON data_access_grant
BEGIN SELECT RAISE(ABORT,'data_access_grant_immutable'); END;
