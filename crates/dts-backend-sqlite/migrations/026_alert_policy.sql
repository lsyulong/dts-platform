CREATE TABLE IF NOT EXISTS alert_policy_evaluation (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    policy_kind TEXT NOT NULL DEFAULT 'alert_policy' CHECK(policy_kind='alert_policy'),
    policy_id TEXT NOT NULL,
    policy_version INTEGER NOT NULL CHECK(policy_version>0),
    event_type TEXT NOT NULL CHECK(event_type IN('task_failure','quality_failure','system_health')),
    window_start TEXT NOT NULL,
    window_end TEXT NOT NULL,
    observed_count INTEGER NOT NULL CHECK(observed_count>=0),
    threshold INTEGER NOT NULL CHECK(threshold BETWEEN 1 AND 1000000),
    state TEXT NOT NULL CHECK(state IN('triggered','within_threshold','suppressed','test')),
    cooldown_until TEXT,
    actor_id TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    request_hash TEXT NOT NULL CHECK(length(request_hash)=64),
    created_at TEXT NOT NULL,
    UNIQUE(tenant_id,id),
    UNIQUE(tenant_id,idempotency_key),
    FOREIGN KEY(tenant_id,policy_kind,policy_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,actor_id) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);
CREATE INDEX IF NOT EXISTS alert_policy_evaluation_list ON alert_policy_evaluation(tenant_id,policy_id,created_at DESC,id);

CREATE TABLE IF NOT EXISTS alert_policy_delivery (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    evaluation_id TEXT NOT NULL,
    recipient_id TEXT NOT NULL,
    subject TEXT NOT NULL,
    body TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN('pending','delivered','failed')),
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK(attempt_count>=0),
    last_error TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(tenant_id,evaluation_id,recipient_id),
    FOREIGN KEY(tenant_id,evaluation_id) REFERENCES alert_policy_evaluation(tenant_id,id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,recipient_id) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);
CREATE INDEX IF NOT EXISTS alert_policy_delivery_outbox ON alert_policy_delivery(tenant_id,status,created_at,id);
CREATE INDEX IF NOT EXISTS alert_policy_task_failure_source ON task_run(status,end_time,created_at);
CREATE INDEX IF NOT EXISTS alert_policy_quality_failure_source ON quality_observation(tenant_id,state,created_at);
CREATE INDEX IF NOT EXISTS alert_policy_health_source ON datasource(tenant_id,health_status,last_test_at);

CREATE TRIGGER IF NOT EXISTS alert_policy_evaluation_no_update BEFORE UPDATE ON alert_policy_evaluation BEGIN SELECT RAISE(ABORT,'immutable alert policy evaluation'); END;
CREATE TRIGGER IF NOT EXISTS alert_policy_evaluation_no_delete BEFORE DELETE ON alert_policy_evaluation BEGIN SELECT RAISE(ABORT,'immutable alert policy evaluation'); END;
