CREATE TABLE IF NOT EXISTS runtime_configuration_publication (
    tenant_id TEXT NOT NULL,
    configuration_kind TEXT NOT NULL DEFAULT 'development_config' CHECK(configuration_kind='development_config'),
    configuration_id TEXT NOT NULL,
    configuration_version BIGINT NOT NULL CHECK(configuration_version>0),
    definition_hash TEXT NOT NULL CHECK(length(definition_hash)=64),
    snapshot TEXT NOT NULL CHECK(jsonb_typeof(snapshot::jsonb)='object' AND octet_length(snapshot)<=327680),
    CHECK(COALESCE(snapshot::jsonb->>'tenant_id'=tenant_id AND snapshot::jsonb->>'kind'=configuration_kind AND snapshot::jsonb->>'id'=configuration_id AND jsonb_typeof(snapshot::jsonb->'version')='number' AND snapshot::jsonb->>'version'=configuration_version::text,FALSE)),
    PRIMARY KEY(tenant_id,configuration_id,configuration_version),
    FOREIGN KEY(tenant_id,configuration_kind,configuration_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT
);
CREATE TABLE IF NOT EXISTS runtime_source_configuration_binding (
    tenant_id TEXT NOT NULL,
    source_kind TEXT NOT NULL CHECK(source_kind IN ('sql_document','development_workflow')),
    source_id TEXT NOT NULL,
    source_version BIGINT NOT NULL CHECK(source_version>0),
    source_hash TEXT NOT NULL CHECK(length(source_hash)=64),
    configuration_id TEXT NOT NULL,
    configuration_version BIGINT NOT NULL,
    evidence TEXT NOT NULL CHECK(jsonb_typeof(evidence::jsonb)='object' AND octet_length(evidence)<=4096),
    CHECK(COALESCE(evidence::jsonb->>'tenant_id'=tenant_id AND evidence::jsonb->>'configuration_id'=configuration_id AND evidence::jsonb->>'configuration_version'=configuration_version::text AND jsonb_typeof(evidence::jsonb->'configuration_version')='number',FALSE)),
    PRIMARY KEY(tenant_id,source_kind,source_id,source_version),
    FOREIGN KEY(tenant_id,source_kind,source_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,configuration_id,configuration_version) REFERENCES runtime_configuration_publication(tenant_id,configuration_id,configuration_version) ON DELETE RESTRICT
);
CREATE TABLE IF NOT EXISTS runtime_job_configuration_binding (
    tenant_id TEXT NOT NULL,
    job_id TEXT NOT NULL,
    configuration_id TEXT NOT NULL,
    configuration_version BIGINT NOT NULL,
    concurrency INTEGER NOT NULL CHECK(concurrency BETWEEN 1 AND 16),
    evidence TEXT NOT NULL CHECK(jsonb_typeof(evidence::jsonb)='object' AND octet_length(evidence)<=4096),
    CHECK(COALESCE(evidence::jsonb->>'tenant_id'=tenant_id AND evidence::jsonb->>'configuration_id'=configuration_id AND evidence::jsonb->>'configuration_version'=configuration_version::text AND jsonb_typeof(evidence::jsonb->'configuration_version')='number' AND evidence::jsonb->>'concurrency'=concurrency::text AND jsonb_typeof(evidence::jsonb->'concurrency')='number',FALSE)),
    PRIMARY KEY(tenant_id,job_id),
    FOREIGN KEY(tenant_id,job_id) REFERENCES product_job(tenant_id,id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,configuration_id,configuration_version) REFERENCES runtime_configuration_publication(tenant_id,configuration_id,configuration_version) ON DELETE RESTRICT
);
CREATE INDEX IF NOT EXISTS idx_runtime_job_configuration ON runtime_job_configuration_binding(tenant_id,configuration_id);
CREATE TABLE IF NOT EXISTS runtime_configuration_receipt (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    action TEXT NOT NULL CHECK(action IN ('publish','withdraw')),
    semantic TEXT NOT NULL CHECK(jsonb_typeof(semantic::jsonb)='object'),
    result_snapshot TEXT NOT NULL CHECK(jsonb_typeof(result_snapshot::jsonb)='object' AND octet_length(result_snapshot)<=393216),
    UNIQUE(tenant_id,idempotency_key),
    FOREIGN KEY(tenant_id,actor_id) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);
CREATE OR REPLACE FUNCTION reject_runtime_configuration_mutation() RETURNS TRIGGER AS $$
BEGIN RAISE EXCEPTION 'immutable runtime configuration evidence'; END;
$$ LANGUAGE plpgsql;
DROP TRIGGER IF EXISTS runtime_configuration_publication_immutable ON runtime_configuration_publication;
CREATE TRIGGER runtime_configuration_publication_immutable BEFORE UPDATE OR DELETE ON runtime_configuration_publication FOR EACH ROW EXECUTE FUNCTION reject_runtime_configuration_mutation();
DROP TRIGGER IF EXISTS runtime_source_configuration_binding_immutable ON runtime_source_configuration_binding;
CREATE TRIGGER runtime_source_configuration_binding_immutable BEFORE UPDATE OR DELETE ON runtime_source_configuration_binding FOR EACH ROW EXECUTE FUNCTION reject_runtime_configuration_mutation();
DROP TRIGGER IF EXISTS runtime_job_configuration_binding_immutable ON runtime_job_configuration_binding;
CREATE TRIGGER runtime_job_configuration_binding_immutable BEFORE UPDATE OR DELETE ON runtime_job_configuration_binding FOR EACH ROW EXECUTE FUNCTION reject_runtime_configuration_mutation();
DROP TRIGGER IF EXISTS runtime_configuration_receipt_immutable ON runtime_configuration_receipt;
CREATE TRIGGER runtime_configuration_receipt_immutable BEFORE UPDATE OR DELETE ON runtime_configuration_receipt FOR EACH ROW EXECUTE FUNCTION reject_runtime_configuration_mutation();
