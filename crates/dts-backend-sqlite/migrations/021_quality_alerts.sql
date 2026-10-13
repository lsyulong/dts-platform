CREATE TABLE IF NOT EXISTS quality_observation (
 id TEXT NOT NULL, tenant_id TEXT NOT NULL, monitor_kind TEXT NOT NULL DEFAULT 'quality_monitor' CHECK(monitor_kind='quality_monitor'),
 monitor_id TEXT NOT NULL, monitor_version INTEGER NOT NULL CHECK(monitor_version>0),
 object_kind TEXT NOT NULL DEFAULT 'quality_object' CHECK(object_kind='quality_object'), object_id TEXT NOT NULL,
 actor_id TEXT NOT NULL, period_start TEXT NOT NULL, period_end TEXT NOT NULL,
 state TEXT NOT NULL CHECK(state IN('threshold_exceeded','within_threshold','unavailable')),
 payload TEXT NOT NULL CHECK(json_valid(payload) AND length(CAST(payload AS BLOB))<=550000), created_at TEXT NOT NULL,
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
 opening_observation_id TEXT NOT NULL,last_observation_id TEXT NOT NULL,payload TEXT NOT NULL CHECK(json_valid(payload)),updated_at TEXT NOT NULL,
 PRIMARY KEY(tenant_id,id), FOREIGN KEY(tenant_id,monitor_kind,monitor_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT,
 FOREIGN KEY(tenant_id,opening_observation_id) REFERENCES quality_observation(tenant_id,id) ON DELETE RESTRICT,
 FOREIGN KEY(tenant_id,last_observation_id) REFERENCES quality_observation(tenant_id,id) ON DELETE RESTRICT
);
CREATE UNIQUE INDEX IF NOT EXISTS quality_alert_live ON quality_alert(tenant_id,monitor_id) WHERE status IN('open','acknowledged');
CREATE INDEX IF NOT EXISTS quality_alert_list ON quality_alert(tenant_id,monitor_id,status,updated_at DESC,id);
CREATE TABLE IF NOT EXISTS quality_alert_event (
 id TEXT NOT NULL,tenant_id TEXT NOT NULL,alert_id TEXT NOT NULL,observation_id TEXT NOT NULL,actor_id TEXT NOT NULL,
 payload TEXT NOT NULL CHECK(json_valid(payload)),created_at TEXT NOT NULL,PRIMARY KEY(tenant_id,id),
 FOREIGN KEY(tenant_id,alert_id) REFERENCES quality_alert(tenant_id,id) ON DELETE RESTRICT,
 FOREIGN KEY(tenant_id,observation_id) REFERENCES quality_observation(tenant_id,id) ON DELETE RESTRICT,
 FOREIGN KEY(tenant_id,actor_id) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);
CREATE INDEX IF NOT EXISTS quality_alert_event_list ON quality_alert_event(tenant_id,alert_id,created_at DESC,id);
CREATE TABLE IF NOT EXISTS quality_alert_receipt (
 tenant_id TEXT NOT NULL,actor_id TEXT NOT NULL,idempotency_key TEXT NOT NULL,
 semantics TEXT NOT NULL CHECK(json_valid(semantics)),result TEXT NOT NULL CHECK(json_valid(result)),
 PRIMARY KEY(tenant_id,actor_id,idempotency_key),FOREIGN KEY(tenant_id,actor_id) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);
CREATE TABLE IF NOT EXISTS quality_alert_notice (
 id TEXT NOT NULL,tenant_id TEXT NOT NULL,alert_id TEXT NOT NULL,recipient_id TEXT NOT NULL,
 subject TEXT NOT NULL,link TEXT NOT NULL,delivery_status TEXT NOT NULL DEFAULT 'pending' CHECK(delivery_status IN('pending','unconfigured','failed','delivered')),
 read_at TEXT,created_at TEXT NOT NULL,PRIMARY KEY(tenant_id,id),UNIQUE(tenant_id,alert_id,recipient_id),
 FOREIGN KEY(tenant_id,alert_id) REFERENCES quality_alert(tenant_id,id) ON DELETE RESTRICT,
 FOREIGN KEY(tenant_id,recipient_id) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);
CREATE INDEX IF NOT EXISTS quality_alert_notice_inbox ON quality_alert_notice(tenant_id,recipient_id,read_at,created_at DESC,id);
CREATE TRIGGER IF NOT EXISTS quality_observation_immutable_update BEFORE UPDATE ON quality_observation BEGIN SELECT RAISE(ABORT,'immutable quality observation'); END;
CREATE TRIGGER IF NOT EXISTS quality_observation_immutable_delete BEFORE DELETE ON quality_observation BEGIN SELECT RAISE(ABORT,'immutable quality observation'); END;
CREATE TRIGGER IF NOT EXISTS quality_alert_event_immutable_update BEFORE UPDATE ON quality_alert_event BEGIN SELECT RAISE(ABORT,'immutable quality event'); END;
CREATE TRIGGER IF NOT EXISTS quality_alert_event_immutable_delete BEFORE DELETE ON quality_alert_event BEGIN SELECT RAISE(ABORT,'immutable quality event'); END;
CREATE TRIGGER IF NOT EXISTS quality_alert_receipt_immutable_update BEFORE UPDATE ON quality_alert_receipt BEGIN SELECT RAISE(ABORT,'immutable quality receipt'); END;
CREATE TRIGGER IF NOT EXISTS quality_alert_receipt_immutable_delete BEFORE DELETE ON quality_alert_receipt BEGIN SELECT RAISE(ABORT,'immutable quality receipt'); END;
