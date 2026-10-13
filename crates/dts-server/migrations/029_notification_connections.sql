CREATE TABLE IF NOT EXISTS notification_connection (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    name TEXT NOT NULL CHECK(char_length(btrim(name)) BETWEEN 1 AND 120),
    channel TEXT NOT NULL CHECK(channel IN ('email','webhook','feishu','dingtalk','wechat_work')),
    public_config JSONB NOT NULL,
    encrypted_secret TEXT,
    enabled BOOLEAN NOT NULL,
    version BIGINT NOT NULL DEFAULT 1 CHECK(version >= 1),
    last_test_status TEXT CHECK(last_test_status IS NULL OR last_test_status IN ('success','failed')),
    last_test_error TEXT CHECK(last_test_error IS NULL OR char_length(last_test_error) <= 2048),
    last_test_at TIMESTAMPTZ,
    created_by TEXT NOT NULL,
    updated_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    deleted_at TIMESTAMPTZ,
    FOREIGN KEY(tenant_id) REFERENCES tenant(id) ON DELETE CASCADE,
    UNIQUE(tenant_id,name),
    UNIQUE(tenant_id,id)
);

CREATE INDEX IF NOT EXISTS notification_connection_active
    ON notification_connection(tenant_id,enabled,name) WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS notification_connection_audit (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    connection_id TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    operation TEXT NOT NULL CHECK(operation IN ('create','update','enable','disable','delete','test')),
    version BIGINT NOT NULL,
    summary JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    FOREIGN KEY(tenant_id) REFERENCES tenant(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS notification_connection_audit_history
    ON notification_connection_audit(tenant_id,connection_id,created_at DESC,id DESC);

CREATE OR REPLACE FUNCTION reject_notification_connection_audit_change()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'notification connection audit is immutable';
END;
$$;
DROP TRIGGER IF EXISTS notification_connection_audit_no_change ON notification_connection_audit;
CREATE TRIGGER notification_connection_audit_no_change
BEFORE UPDATE OR DELETE ON notification_connection_audit
FOR EACH ROW EXECUTE FUNCTION reject_notification_connection_audit_change();
