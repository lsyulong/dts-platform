CREATE UNIQUE INDEX IF NOT EXISTS datasource_tenant_identity
    ON datasource(tenant_id,id);

CREATE TABLE IF NOT EXISTS datasource_probe (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    datasource_id TEXT NOT NULL,
    supported INTEGER NOT NULL CHECK(supported IN (0,1)),
    status TEXT NOT NULL CHECK(status IN ('healthy','unhealthy','unknown')),
    latency_ms INTEGER CHECK(latency_ms IS NULL OR latency_ms >= 0),
    error_message TEXT CHECK(error_message IS NULL OR length(error_message) <= 2048),
    actor_id TEXT NOT NULL,
    observed_at TEXT NOT NULL,
    FOREIGN KEY(tenant_id,datasource_id) REFERENCES datasource(tenant_id,id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS datasource_probe_history
    ON datasource_probe(tenant_id,datasource_id,observed_at DESC,id DESC);

CREATE TRIGGER IF NOT EXISTS datasource_probe_no_update
BEFORE UPDATE ON datasource_probe
BEGIN
    SELECT RAISE(ABORT, 'datasource probe evidence is immutable');
END;
