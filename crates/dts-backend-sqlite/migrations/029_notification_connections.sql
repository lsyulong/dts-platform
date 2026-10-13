CREATE TABLE IF NOT EXISTS notification_connection (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    name TEXT NOT NULL CHECK(length(trim(name)) BETWEEN 1 AND 120),
    channel TEXT NOT NULL CHECK(channel IN ('email','webhook','feishu','dingtalk','wechat_work')),
    public_config TEXT NOT NULL CHECK(json_valid(public_config)),
    encrypted_secret TEXT,
    enabled INTEGER NOT NULL CHECK(enabled IN (0,1)),
    version INTEGER NOT NULL DEFAULT 1 CHECK(version >= 1),
    last_test_status TEXT CHECK(last_test_status IS NULL OR last_test_status IN ('success','failed')),
    last_test_error TEXT CHECK(last_test_error IS NULL OR length(last_test_error) <= 2048),
    last_test_at TEXT,
    created_by TEXT NOT NULL,
    updated_by TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT,
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
    version INTEGER NOT NULL,
    summary TEXT NOT NULL CHECK(json_valid(summary)),
    created_at TEXT NOT NULL,
    FOREIGN KEY(tenant_id) REFERENCES tenant(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS notification_connection_audit_history
    ON notification_connection_audit(tenant_id,connection_id,created_at DESC,id DESC);

CREATE TRIGGER IF NOT EXISTS notification_connection_audit_no_update
BEFORE UPDATE ON notification_connection_audit BEGIN
    SELECT RAISE(ABORT, 'notification connection audit is immutable');
END;
CREATE TRIGGER IF NOT EXISTS notification_connection_audit_no_delete
BEFORE DELETE ON notification_connection_audit BEGIN
    SELECT RAISE(ABORT, 'notification connection audit is immutable');
END;
