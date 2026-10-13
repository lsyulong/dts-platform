CREATE UNIQUE INDEX IF NOT EXISTS idx_classification_rank ON product_resource(tenant_id, CAST(json_extract(properties,'$.rank') AS INTEGER))
    WHERE kind='data_classification' AND json_type(properties,'$.rank')='integer';
CREATE TABLE IF NOT EXISTS sensitive_finding (
    id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL,
    finding_kind TEXT NOT NULL DEFAULT 'sensitive_finding' CHECK(finding_kind='sensitive_finding'),
    task_kind TEXT NOT NULL DEFAULT 'recognition_task' CHECK(task_kind='recognition_task'), task_id TEXT NOT NULL, task_version INTEGER NOT NULL CHECK(task_version>0),
    object_kind TEXT NOT NULL DEFAULT 'metadata_object' CHECK(object_kind='metadata_object'), object_id TEXT NOT NULL, object_version INTEGER NOT NULL CHECK(object_version>0),
    rule_kind TEXT NOT NULL DEFAULT 'recognition_rule' CHECK(rule_kind='recognition_rule'), rule_id TEXT NOT NULL, rule_version INTEGER NOT NULL CHECK(rule_version>0),
    classification_kind TEXT NOT NULL DEFAULT 'data_classification' CHECK(classification_kind='data_classification'), classification_id TEXT NOT NULL, classification_version INTEGER NOT NULL CHECK(classification_version>0),
    job_id TEXT NOT NULL, node_id TEXT NOT NULL, column_name TEXT NOT NULL,
    evidence TEXT NOT NULL CHECK(json_valid(evidence)), evaluation_scope TEXT NOT NULL CHECK(evaluation_scope IN('sample','column_names')),
    sampled_rows INTEGER NOT NULL CHECK(sampled_rows BETWEEN 0 AND 1000), definition_hash TEXT NOT NULL CHECK(length(definition_hash)=64),
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN('pending','confirmed','dismissed')), version INTEGER NOT NULL DEFAULT 1 CHECK(version>0),
    reviewed_by TEXT, review_reason TEXT, reviewed_at TEXT, created_at TEXT NOT NULL DEFAULT(strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE(tenant_id,id), UNIQUE(tenant_id,job_id,node_id,column_name),
    FOREIGN KEY(tenant_id,finding_kind,id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,task_kind,task_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,object_kind,object_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,rule_kind,rule_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,classification_kind,classification_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,job_id,node_id) REFERENCES product_job_node(tenant_id,job_id,node_id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,reviewed_by) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT,
    CHECK((status='pending' AND reviewed_by IS NULL AND reviewed_at IS NULL AND review_reason IS NULL)
        OR(status<>'pending' AND reviewed_by IS NOT NULL AND reviewed_at IS NOT NULL AND length(trim(review_reason))>0))
);
CREATE INDEX IF NOT EXISTS idx_sensitive_findings ON sensitive_finding(tenant_id,status,created_at DESC);
CREATE TABLE IF NOT EXISTS sensitive_finding_audit (
    id TEXT PRIMARY KEY,tenant_id TEXT NOT NULL,finding_id TEXT NOT NULL,actor_id TEXT NOT NULL,
    operation TEXT NOT NULL CHECK(operation IN('discovered','confirmed','dismissed')),reason TEXT,
    created_at TEXT NOT NULL DEFAULT(strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    FOREIGN KEY(tenant_id,finding_id) REFERENCES sensitive_finding(tenant_id,id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,actor_id) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);
