CREATE UNIQUE INDEX IF NOT EXISTS idx_master_actor_tenant_id ON "user"(tenant_id,id);

CREATE TABLE IF NOT EXISTS master_model_schema (
    tenant_id TEXT NOT NULL,
    model_id TEXT NOT NULL,
    model_kind TEXT NOT NULL DEFAULT 'master_data_model' CHECK (model_kind = 'master_data_model'),
    model_version BIGINT NOT NULL CHECK (model_version > 0),
    schema_snapshot JSONB NOT NULL,
    published_by TEXT NOT NULL,
    published_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (tenant_id, model_id, model_version),
    FOREIGN KEY (tenant_id, model_kind, model_id) REFERENCES product_resource(tenant_id, kind, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, published_by) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS master_record_identity (
    tenant_id TEXT NOT NULL,
    record_id TEXT NOT NULL,
    record_kind TEXT NOT NULL DEFAULT 'master_data_record' CHECK (record_kind = 'master_data_record'),
    model_id TEXT NOT NULL,
    model_version BIGINT NOT NULL,
    business_key TEXT NOT NULL,
    key_fingerprint TEXT NOT NULL,
    submitted_by TEXT,
    approved_version BIGINT,
    PRIMARY KEY (tenant_id, record_id),
    UNIQUE (tenant_id, model_id, business_key),
    UNIQUE (tenant_id, model_id, key_fingerprint),
    FOREIGN KEY (tenant_id, record_kind, record_id) REFERENCES product_resource(tenant_id, kind, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, model_id, model_version) REFERENCES master_model_schema(tenant_id, model_id, model_version) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, submitted_by) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS master_record_revision (
    tenant_id TEXT NOT NULL,
    record_id TEXT NOT NULL,
    record_version BIGINT NOT NULL,
    model_id TEXT NOT NULL,
    model_version BIGINT NOT NULL,
    business_key TEXT NOT NULL,
    record_values JSONB NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('draft', 'pending', 'approved', 'rejected')),
    actor_id TEXT NOT NULL,
    reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
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
    width BIGINT NOT NULL CHECK (width BETWEEN 1 AND 18),
    start_value BIGINT NOT NULL CHECK (start_value >= 0),
    next_value BIGINT NOT NULL CHECK (next_value >= 0),
    exhausted BOOLEAN NOT NULL DEFAULT FALSE,
    allocated_count BIGINT NOT NULL DEFAULT 0 CHECK (allocated_count >= 0),
    PRIMARY KEY (tenant_id, rule_id),
    UNIQUE (tenant_id, model_id, prefix),
    FOREIGN KEY (tenant_id, rule_kind, rule_id) REFERENCES product_resource(tenant_id, kind, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, model_kind, model_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS master_code_allocation (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    rule_id TEXT NOT NULL,
    rule_version BIGINT NOT NULL,
    model_id TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    sequence BIGINT NOT NULL,
    code TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (tenant_id, idempotency_key),
    UNIQUE (tenant_id, model_id, code),
    UNIQUE (tenant_id, rule_id, sequence),
    FOREIGN KEY (tenant_id, rule_id) REFERENCES master_code_sequence(tenant_id, rule_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, actor_id) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);

ALTER TABLE master_record_identity ADD CONSTRAINT fk_master_approved_revision
    FOREIGN KEY (tenant_id, record_id, approved_version) REFERENCES master_record_revision(tenant_id, record_id, record_version) DEFERRABLE INITIALLY DEFERRED;

CREATE OR REPLACE FUNCTION master_snapshot_immutable() RETURNS trigger AS $$
BEGIN
    IF TG_TABLE_NAME='master_record_revision' AND TG_OP='DELETE' THEN
        IF OLD.status='draft' THEN RETURN OLD; END IF;
    END IF;
    RAISE EXCEPTION 'master_snapshot_immutable' USING ERRCODE='23514';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER master_schema_immutable BEFORE UPDATE OR DELETE ON master_model_schema
FOR EACH ROW EXECUTE FUNCTION master_snapshot_immutable();
CREATE TRIGGER master_revision_immutable BEFORE UPDATE OR DELETE ON master_record_revision
FOR EACH ROW EXECUTE FUNCTION master_snapshot_immutable();
