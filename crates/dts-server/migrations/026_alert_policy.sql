CREATE TABLE IF NOT EXISTS alert_policy_evaluation (
    id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    policy_kind TEXT NOT NULL DEFAULT 'alert_policy' CHECK(policy_kind='alert_policy'),
    policy_id TEXT NOT NULL,
    policy_version BIGINT NOT NULL CHECK(policy_version>0),
    event_type TEXT NOT NULL CHECK(event_type IN('task_failure','quality_failure','system_health')),
    window_start TIMESTAMPTZ NOT NULL,
    window_end TIMESTAMPTZ NOT NULL,
    observed_count BIGINT NOT NULL CHECK(observed_count>=0),
    threshold BIGINT NOT NULL CHECK(threshold BETWEEN 1 AND 1000000),
    state TEXT NOT NULL CHECK(state IN('triggered','within_threshold','suppressed','test')),
    cooldown_until TIMESTAMPTZ,
    actor_id TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    request_hash TEXT NOT NULL CHECK(length(request_hash)=64),
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY(tenant_id,id),
    UNIQUE(tenant_id,idempotency_key),
    FOREIGN KEY(tenant_id,policy_kind,policy_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,actor_id) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);
CREATE INDEX IF NOT EXISTS alert_policy_evaluation_list ON alert_policy_evaluation(tenant_id,policy_id,created_at DESC,id);

CREATE TABLE IF NOT EXISTS alert_policy_delivery (
    id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    evaluation_id TEXT NOT NULL,
    recipient_id TEXT NOT NULL,
    subject TEXT NOT NULL,
    body TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN('pending','delivered','failed')),
    attempt_count BIGINT NOT NULL DEFAULT 0 CHECK(attempt_count>=0),
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY(tenant_id,id),
    UNIQUE(tenant_id,evaluation_id,recipient_id),
    FOREIGN KEY(tenant_id,evaluation_id) REFERENCES alert_policy_evaluation(tenant_id,id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,recipient_id) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);
CREATE INDEX IF NOT EXISTS alert_policy_delivery_outbox ON alert_policy_delivery(tenant_id,status,created_at,id);
CREATE INDEX IF NOT EXISTS alert_policy_task_failure_source ON task_run(status,end_time,created_at);
CREATE INDEX IF NOT EXISTS alert_policy_quality_failure_source ON quality_observation(tenant_id,state,created_at);
CREATE INDEX IF NOT EXISTS alert_policy_health_source ON datasource(tenant_id,health_status,last_test_at);

CREATE OR REPLACE FUNCTION reject_alert_policy_evaluation_mutation() RETURNS TRIGGER AS $$
BEGIN RAISE EXCEPTION 'immutable alert policy evaluation'; END;
$$ LANGUAGE plpgsql;
DROP TRIGGER IF EXISTS alert_policy_evaluation_immutable ON alert_policy_evaluation;
CREATE TRIGGER alert_policy_evaluation_immutable BEFORE UPDATE OR DELETE ON alert_policy_evaluation FOR EACH ROW EXECUTE FUNCTION reject_alert_policy_evaluation_mutation();
