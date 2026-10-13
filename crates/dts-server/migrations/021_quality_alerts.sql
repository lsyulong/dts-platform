CREATE TABLE IF NOT EXISTS quality_observation (
 id TEXT NOT NULL, tenant_id TEXT NOT NULL, monitor_kind TEXT NOT NULL DEFAULT 'quality_monitor' CHECK(monitor_kind='quality_monitor'),
 monitor_id TEXT NOT NULL, monitor_version INTEGER NOT NULL CHECK(monitor_version>0),
 object_kind TEXT NOT NULL DEFAULT 'quality_object' CHECK(object_kind='quality_object'), object_id TEXT NOT NULL,
 actor_id TEXT NOT NULL, period_start TIMESTAMPTZ NOT NULL, period_end TIMESTAMPTZ NOT NULL,
 state TEXT NOT NULL CHECK(state IN('threshold_exceeded','within_threshold','unavailable')),
 payload TEXT NOT NULL CHECK(jsonb_typeof(payload::jsonb)='object' AND octet_length(payload)<=550000), created_at TIMESTAMPTZ NOT NULL,
 PRIMARY KEY(tenant_id,id), FOREIGN KEY(tenant_id,monitor_kind,monitor_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT,
 FOREIGN KEY(tenant_id,object_kind,object_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT,
 FOREIGN KEY(tenant_id,actor_id) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);
CREATE INDEX IF NOT EXISTS quality_observation_list ON quality_observation(tenant_id,monitor_id,created_at DESC,id);
CREATE TABLE IF NOT EXISTS quality_observation_resource (
 tenant_id TEXT NOT NULL, observation_id TEXT NOT NULL, kind TEXT NOT NULL, resource_id TEXT NOT NULL,
 PRIMARY KEY(tenant_id,observation_id,kind,resource_id),
 FOREIGN KEY(tenant_id,observation_id) REFERENCES quality_observation(tenant_id,id) ON DELETE RESTRICT,
 FOREIGN KEY(tenant_id,kind,resource_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT
);
CREATE TABLE IF NOT EXISTS quality_observation_job (
 tenant_id TEXT NOT NULL, observation_id TEXT NOT NULL, job_id TEXT NOT NULL,
 PRIMARY KEY(tenant_id,observation_id,job_id),
 FOREIGN KEY(tenant_id,observation_id) REFERENCES quality_observation(tenant_id,id) ON DELETE RESTRICT,
 FOREIGN KEY(tenant_id,job_id) REFERENCES product_job(tenant_id,id) ON DELETE RESTRICT
);
CREATE TABLE IF NOT EXISTS quality_alert (
 id TEXT NOT NULL, tenant_id TEXT NOT NULL, monitor_id TEXT NOT NULL, monitor_kind TEXT NOT NULL DEFAULT 'quality_monitor' CHECK(monitor_kind='quality_monitor'),
 status TEXT NOT NULL CHECK(status IN('open','acknowledged','resolved')), version INTEGER NOT NULL CHECK(version>0),
 opening_observation_id TEXT NOT NULL,last_observation_id TEXT NOT NULL,payload TEXT NOT NULL CHECK(jsonb_typeof(payload::jsonb)='object'),updated_at TIMESTAMPTZ NOT NULL,
 PRIMARY KEY(tenant_id,id), FOREIGN KEY(tenant_id,monitor_kind,monitor_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT,
 FOREIGN KEY(tenant_id,opening_observation_id) REFERENCES quality_observation(tenant_id,id) ON DELETE RESTRICT,
 FOREIGN KEY(tenant_id,last_observation_id) REFERENCES quality_observation(tenant_id,id) ON DELETE RESTRICT
);
CREATE UNIQUE INDEX IF NOT EXISTS quality_alert_live ON quality_alert(tenant_id,monitor_id) WHERE status IN('open','acknowledged');
CREATE INDEX IF NOT EXISTS quality_alert_list ON quality_alert(tenant_id,monitor_id,status,updated_at DESC,id);
CREATE TABLE IF NOT EXISTS quality_alert_event (
 id TEXT NOT NULL,tenant_id TEXT NOT NULL,alert_id TEXT NOT NULL,observation_id TEXT NOT NULL,actor_id TEXT NOT NULL,
 payload TEXT NOT NULL CHECK(jsonb_typeof(payload::jsonb)='object'),created_at TIMESTAMPTZ NOT NULL,PRIMARY KEY(tenant_id,id),
 FOREIGN KEY(tenant_id,alert_id) REFERENCES quality_alert(tenant_id,id) ON DELETE RESTRICT,
 FOREIGN KEY(tenant_id,observation_id) REFERENCES quality_observation(tenant_id,id) ON DELETE RESTRICT,
 FOREIGN KEY(tenant_id,actor_id) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);
CREATE INDEX IF NOT EXISTS quality_alert_event_list ON quality_alert_event(tenant_id,alert_id,created_at DESC,id);
CREATE TABLE IF NOT EXISTS quality_alert_receipt (
 tenant_id TEXT NOT NULL,actor_id TEXT NOT NULL,idempotency_key TEXT NOT NULL,
 semantics TEXT NOT NULL CHECK(jsonb_typeof(semantics::jsonb)='object'),result TEXT NOT NULL CHECK(jsonb_typeof(result::jsonb)='object'),
 PRIMARY KEY(tenant_id,actor_id,idempotency_key),FOREIGN KEY(tenant_id,actor_id) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);
CREATE TABLE IF NOT EXISTS quality_alert_notice (
 id TEXT NOT NULL,tenant_id TEXT NOT NULL,alert_id TEXT NOT NULL,recipient_id TEXT NOT NULL,
 subject TEXT NOT NULL,link TEXT NOT NULL,delivery_status TEXT NOT NULL DEFAULT 'pending' CHECK(delivery_status IN('pending','unconfigured','failed','delivered')),
 read_at TIMESTAMPTZ,created_at TIMESTAMPTZ NOT NULL,PRIMARY KEY(tenant_id,id),UNIQUE(tenant_id,alert_id,recipient_id),
 FOREIGN KEY(tenant_id,alert_id) REFERENCES quality_alert(tenant_id,id) ON DELETE RESTRICT,
 FOREIGN KEY(tenant_id,recipient_id) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);
CREATE INDEX IF NOT EXISTS quality_alert_notice_inbox ON quality_alert_notice(tenant_id,recipient_id,read_at,created_at DESC,id);
CREATE OR REPLACE FUNCTION quality_alert_immutable() RETURNS trigger AS $$ BEGIN RAISE EXCEPTION 'immutable quality observation/event/receipt'; END; $$ LANGUAGE plpgsql;
DROP TRIGGER IF EXISTS quality_observation_immutable ON quality_observation;
CREATE TRIGGER quality_observation_immutable BEFORE UPDATE OR DELETE ON quality_observation FOR EACH ROW EXECUTE FUNCTION quality_alert_immutable();
DROP TRIGGER IF EXISTS quality_alert_event_immutable ON quality_alert_event;
CREATE TRIGGER quality_alert_event_immutable BEFORE UPDATE OR DELETE ON quality_alert_event FOR EACH ROW EXECUTE FUNCTION quality_alert_immutable();
DROP TRIGGER IF EXISTS quality_alert_receipt_immutable ON quality_alert_receipt;
CREATE TRIGGER quality_alert_receipt_immutable BEFORE UPDATE OR DELETE ON quality_alert_receipt FOR EACH ROW EXECUTE FUNCTION quality_alert_immutable();
