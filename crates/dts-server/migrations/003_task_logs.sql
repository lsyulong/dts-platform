CREATE UNIQUE INDEX IF NOT EXISTS idx_task_run_identity ON task_run(id, task_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_sync_task_identity ON sync_task(id, tenant_id);

CREATE TABLE IF NOT EXISTS task_run_log (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    task_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    sequence BIGINT NOT NULL CHECK(sequence >= 1),
    timestamp TIMESTAMPTZ NOT NULL,
    level TEXT NOT NULL CHECK(level IN ('TRACE','DEBUG','INFO','WARN','ERROR','FATAL')),
    component TEXT NOT NULL CHECK(octet_length(component) BETWEEN 1 AND 64),
    message TEXT NOT NULL CHECK(octet_length(message) <= 8192),
    UNIQUE(run_id, sequence),
    FOREIGN KEY(task_id, tenant_id) REFERENCES sync_task(id, tenant_id) ON DELETE CASCADE,
    FOREIGN KEY(run_id, task_id) REFERENCES task_run(id, task_id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_task_run_log_page ON task_run_log(tenant_id, task_id, run_id, sequence);
