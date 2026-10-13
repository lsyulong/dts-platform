CREATE UNIQUE INDEX IF NOT EXISTS datasource_tenant_identity
    ON datasource(tenant_id,id);

CREATE TABLE IF NOT EXISTS datasource_probe (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    datasource_id TEXT NOT NULL,
    supported BOOLEAN NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('healthy','unhealthy','unknown')),
    latency_ms BIGINT CHECK(latency_ms IS NULL OR latency_ms >= 0),
    error_message TEXT CHECK(error_message IS NULL OR char_length(error_message) <= 2048),
    actor_id TEXT NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL,
    FOREIGN KEY(tenant_id,datasource_id) REFERENCES datasource(tenant_id,id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS datasource_probe_history
    ON datasource_probe(tenant_id,datasource_id,observed_at DESC,id DESC);

CREATE OR REPLACE FUNCTION reject_datasource_probe_update()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'datasource probe evidence is immutable';
END;
$$;

DROP TRIGGER IF EXISTS datasource_probe_no_update ON datasource_probe;
CREATE TRIGGER datasource_probe_no_update
BEFORE UPDATE ON datasource_probe
FOR EACH ROW EXECUTE FUNCTION reject_datasource_probe_update();
