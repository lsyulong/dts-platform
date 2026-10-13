CREATE UNIQUE INDEX IF NOT EXISTS alert_policy_delivery_tenant_id
    ON alert_policy_delivery(tenant_id,id);

CREATE TABLE IF NOT EXISTS alert_policy_delivery_state (
    tenant_id TEXT NOT NULL,
    delivery_id TEXT NOT NULL,
    channel TEXT NOT NULL DEFAULT 'in_app' CHECK(channel IN('in_app','email','sms','wechat_work','dingtalk','webhook')),
    next_attempt_at TIMESTAMPTZ NOT NULL,
    lease_owner TEXT,
    lease_fence BIGINT NOT NULL DEFAULT 0 CHECK(lease_fence>=0),
    lease_expires_at TIMESTAMPTZ,
    delivered_at TIMESTAMPTZ,
    PRIMARY KEY(tenant_id,delivery_id),
    FOREIGN KEY(tenant_id,delivery_id) REFERENCES alert_policy_delivery(tenant_id,id) ON DELETE RESTRICT
);
CREATE INDEX IF NOT EXISTS alert_policy_delivery_claim
    ON alert_policy_delivery_state(next_attempt_at,lease_expires_at,tenant_id,delivery_id);

CREATE TABLE IF NOT EXISTS alert_policy_delivery_read (
    tenant_id TEXT NOT NULL,
    delivery_id TEXT NOT NULL,
    read_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY(tenant_id,delivery_id),
    FOREIGN KEY(tenant_id,delivery_id) REFERENCES alert_policy_delivery(tenant_id,id) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS alert_policy_schedule (
    tenant_id TEXT NOT NULL,
    policy_kind TEXT NOT NULL DEFAULT 'alert_policy' CHECK(policy_kind='alert_policy'),
    policy_id TEXT NOT NULL,
    next_evaluation_at TIMESTAMPTZ NOT NULL,
    lease_owner TEXT,
    lease_fence BIGINT NOT NULL DEFAULT 0 CHECK(lease_fence>=0),
    lease_expires_at TIMESTAMPTZ,
    last_error TEXT,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY(tenant_id,policy_id),
    FOREIGN KEY(tenant_id,policy_kind,policy_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS alert_policy_schedule_claim
    ON alert_policy_schedule(next_evaluation_at,lease_expires_at,tenant_id,policy_id);

