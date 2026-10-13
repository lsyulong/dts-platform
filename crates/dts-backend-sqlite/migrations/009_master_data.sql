CREATE UNIQUE INDEX IF NOT EXISTS idx_master_actor_tenant_id ON "user"(tenant_id,id);

CREATE TABLE IF NOT EXISTS master_model_schema (
    tenant_id TEXT NOT NULL,
    model_id TEXT NOT NULL,
    model_kind TEXT NOT NULL DEFAULT 'master_data_model' CHECK (model_kind = 'master_data_model'),
    model_version INTEGER NOT NULL CHECK (model_version > 0),
    schema_snapshot TEXT NOT NULL CHECK (json_valid(schema_snapshot)),
    published_by TEXT NOT NULL,
    published_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (tenant_id, model_id, model_version),
    FOREIGN KEY (tenant_id, model_kind, model_id) REFERENCES product_resource(tenant_id, kind, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, published_by) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS master_record_identity (
    tenant_id TEXT NOT NULL,
    record_id TEXT NOT NULL,
    record_kind TEXT NOT NULL DEFAULT 'master_data_record' CHECK (record_kind = 'master_data_record'),
    model_id TEXT NOT NULL,
    model_version INTEGER NOT NULL,
    business_key TEXT NOT NULL,
    key_fingerprint TEXT NOT NULL,
    submitted_by TEXT,
    approved_version INTEGER,
    PRIMARY KEY (tenant_id, record_id),
    UNIQUE (tenant_id, model_id, business_key),
    UNIQUE (tenant_id, model_id, key_fingerprint),
    FOREIGN KEY (tenant_id, record_kind, record_id) REFERENCES product_resource(tenant_id, kind, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, model_id, model_version) REFERENCES master_model_schema(tenant_id, model_id, model_version) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, submitted_by) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, record_id, approved_version) REFERENCES master_record_revision(tenant_id, record_id, record_version) DEFERRABLE INITIALLY DEFERRED
);

CREATE TABLE IF NOT EXISTS master_record_revision (
    tenant_id TEXT NOT NULL,
    record_id TEXT NOT NULL,
    record_version INTEGER NOT NULL,
    model_id TEXT NOT NULL,
    model_version INTEGER NOT NULL,
    business_key TEXT NOT NULL,
    record_values TEXT NOT NULL CHECK (json_valid(record_values)),
    status TEXT NOT NULL CHECK (status IN ('draft', 'pending', 'approved', 'rejected')),
    actor_id TEXT NOT NULL,
    reason TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (tenant_id, record_id, record_version),
    FOREIGN KEY (tenant_id, record_id) REFERENCES master_record_identity(tenant_id, record_id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, model_id, model_version) REFERENCES master_model_schema(tenant_id, model_id, model_version) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, actor_id) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS master_code_sequence (
    tenant_id TEXT NOT NULL,
    rule_id TEXT NOT NULL,
    rule_kind TEXT NOT NULL DEFAULT 'master_code_rule' CHECK (rule_kind = 'master_code_rule'),
    model_id TEXT NOT NULL,
    model_kind TEXT NOT NULL DEFAULT 'master_data_model' CHECK (model_kind = 'master_data_model'),
    prefix TEXT NOT NULL,
    width INTEGER NOT NULL CHECK (width BETWEEN 1 AND 18),
    start_value INTEGER NOT NULL CHECK (start_value >= 0),
    next_value INTEGER NOT NULL CHECK (next_value >= 0),
    exhausted INTEGER NOT NULL DEFAULT 0 CHECK (exhausted IN (0,1)),
    allocated_count INTEGER NOT NULL DEFAULT 0 CHECK (allocated_count >= 0),
    PRIMARY KEY (tenant_id, rule_id),
    UNIQUE (tenant_id, model_id, prefix),
    FOREIGN KEY (tenant_id, rule_kind, rule_id) REFERENCES product_resource(tenant_id, kind, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, model_kind, model_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS master_code_allocation (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    rule_id TEXT NOT NULL,
    rule_version INTEGER NOT NULL,
    model_id TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    sequence INTEGER NOT NULL,
    code TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE (tenant_id, idempotency_key),
    UNIQUE (tenant_id, model_id, code),
    UNIQUE (tenant_id, rule_id, sequence),
    FOREIGN KEY (tenant_id, rule_id) REFERENCES master_code_sequence(tenant_id, rule_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, actor_id) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);

CREATE TRIGGER IF NOT EXISTS master_schema_immutable_update BEFORE UPDATE ON master_model_schema
BEGIN SELECT RAISE(ABORT,'master_schema_immutable'); END;
CREATE TRIGGER IF NOT EXISTS master_schema_immutable_delete BEFORE DELETE ON master_model_schema
BEGIN SELECT RAISE(ABORT,'master_schema_immutable'); END;
CREATE TRIGGER IF NOT EXISTS master_revision_immutable_update BEFORE UPDATE ON master_record_revision
BEGIN SELECT RAISE(ABORT,'master_revision_immutable'); END;
CREATE TRIGGER IF NOT EXISTS master_revision_evidence_delete BEFORE DELETE ON master_record_revision
WHEN OLD.status <> 'draft'
BEGIN SELECT RAISE(ABORT,'master_revision_immutable'); END;
