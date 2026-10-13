CREATE TABLE IF NOT EXISTS runtime_configuration_publication (
    tenant_id TEXT NOT NULL,
    configuration_kind TEXT NOT NULL DEFAULT 'development_config' CHECK(configuration_kind='development_config'),
    configuration_id TEXT NOT NULL,
    configuration_version INTEGER NOT NULL CHECK(configuration_version>0),
    definition_hash TEXT NOT NULL CHECK(length(definition_hash)=64),
    snapshot TEXT NOT NULL CHECK(json_valid(snapshot) AND length(CAST(snapshot AS BLOB))<=327680),
    CHECK(json_type(snapshot) IS 'object' AND json_extract(snapshot,'$.tenant_id') IS tenant_id AND json_extract(snapshot,'$.kind') IS configuration_kind AND json_extract(snapshot,'$.id') IS configuration_id AND json_type(snapshot,'$.version') IS 'integer' AND json_extract(snapshot,'$.version') IS configuration_version),
    PRIMARY KEY(tenant_id,configuration_id,configuration_version),
    FOREIGN KEY(tenant_id,configuration_kind,configuration_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT
);
CREATE TABLE IF NOT EXISTS runtime_source_configuration_binding (
    tenant_id TEXT NOT NULL,
    source_kind TEXT NOT NULL CHECK(source_kind IN ('sql_document','development_workflow')),
    source_id TEXT NOT NULL,
    source_version INTEGER NOT NULL CHECK(source_version>0),
    source_hash TEXT NOT NULL CHECK(length(source_hash)=64),
    configuration_id TEXT NOT NULL,
    configuration_version INTEGER NOT NULL,
    evidence TEXT NOT NULL CHECK(json_valid(evidence) AND length(CAST(evidence AS BLOB))<=4096),
    CHECK(json_type(evidence) IS 'object' AND json_extract(evidence,'$.tenant_id') IS tenant_id AND json_extract(evidence,'$.configuration_id') IS configuration_id AND json_type(evidence,'$.configuration_version') IS 'integer' AND json_extract(evidence,'$.configuration_version') IS configuration_version),
    PRIMARY KEY(tenant_id,source_kind,source_id,source_version),
    FOREIGN KEY(tenant_id,source_kind,source_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,configuration_id,configuration_version) REFERENCES runtime_configuration_publication(tenant_id,configuration_id,configuration_version) ON DELETE RESTRICT
);
CREATE TABLE IF NOT EXISTS runtime_job_configuration_binding (
    tenant_id TEXT NOT NULL,
    job_id TEXT NOT NULL,
    configuration_id TEXT NOT NULL,
    configuration_version INTEGER NOT NULL,
    concurrency INTEGER NOT NULL CHECK(concurrency BETWEEN 1 AND 16),
    evidence TEXT NOT NULL CHECK(json_valid(evidence) AND length(CAST(evidence AS BLOB))<=4096),
    CHECK(json_type(evidence) IS 'object' AND json_extract(evidence,'$.tenant_id') IS tenant_id AND json_extract(evidence,'$.configuration_id') IS configuration_id AND json_type(evidence,'$.configuration_version') IS 'integer' AND json_extract(evidence,'$.configuration_version') IS configuration_version AND json_type(evidence,'$.concurrency') IS 'integer' AND json_extract(evidence,'$.concurrency') IS concurrency),
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
    semantic TEXT NOT NULL CHECK(json_valid(semantic)),
    result_snapshot TEXT NOT NULL CHECK(json_valid(result_snapshot) AND length(CAST(result_snapshot AS BLOB))<=393216),
    UNIQUE(tenant_id,idempotency_key),
    FOREIGN KEY(tenant_id,actor_id) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);
CREATE TRIGGER IF NOT EXISTS runtime_configuration_publication_no_update BEFORE UPDATE ON runtime_configuration_publication BEGIN SELECT RAISE(ABORT,'immutable runtime configuration publication'); END;
CREATE TRIGGER IF NOT EXISTS runtime_configuration_publication_no_delete BEFORE DELETE ON runtime_configuration_publication BEGIN SELECT RAISE(ABORT,'immutable runtime configuration publication'); END;
CREATE TRIGGER IF NOT EXISTS runtime_source_configuration_binding_no_update BEFORE UPDATE ON runtime_source_configuration_binding BEGIN SELECT RAISE(ABORT,'immutable runtime source binding'); END;
CREATE TRIGGER IF NOT EXISTS runtime_source_configuration_binding_no_delete BEFORE DELETE ON runtime_source_configuration_binding BEGIN SELECT RAISE(ABORT,'immutable runtime source binding'); END;
CREATE TRIGGER IF NOT EXISTS runtime_job_configuration_binding_no_update BEFORE UPDATE ON runtime_job_configuration_binding BEGIN SELECT RAISE(ABORT,'immutable runtime job binding'); END;
CREATE TRIGGER IF NOT EXISTS runtime_job_configuration_binding_no_delete BEFORE DELETE ON runtime_job_configuration_binding BEGIN SELECT RAISE(ABORT,'immutable runtime job binding'); END;
CREATE TRIGGER IF NOT EXISTS runtime_configuration_receipt_no_update BEFORE UPDATE ON runtime_configuration_receipt BEGIN SELECT RAISE(ABORT,'immutable runtime configuration receipt'); END;
CREATE TRIGGER IF NOT EXISTS runtime_configuration_receipt_no_delete BEFORE DELETE ON runtime_configuration_receipt BEGIN SELECT RAISE(ABORT,'immutable runtime configuration receipt'); END;
