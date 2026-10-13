CREATE UNIQUE INDEX IF NOT EXISTS idx_service_user_tenant_identity ON "user"(tenant_id,id);
CREATE TABLE IF NOT EXISTS service_application (
    tenant_id TEXT NOT NULL,
    application_id TEXT NOT NULL,
    resource_kind TEXT NOT NULL DEFAULT 'service_application' CHECK(resource_kind='service_application'),
    resource_version BIGINT NOT NULL CHECK(resource_version>0),
    owner_id TEXT NOT NULL,
    owner_status TEXT NOT NULL,
    allowed_origins TEXT NOT NULL CHECK(jsonb_typeof(allowed_origins::jsonb)='array'),
    credential_expires_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY(tenant_id,application_id),
    FOREIGN KEY(tenant_id,resource_kind,application_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,owner_id,owner_status) REFERENCES "user"(tenant_id,id,status) ON UPDATE CASCADE ON DELETE RESTRICT
);
CREATE INDEX IF NOT EXISTS idx_service_application_owner ON service_application(tenant_id,owner_id,application_id);

CREATE TABLE IF NOT EXISTS service_credential (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    application_id TEXT NOT NULL,
    owner_id TEXT NOT NULL,
    version BIGINT NOT NULL CHECK(version>0),
    verifier TEXT NOT NULL CHECK(length(verifier)=64),
    created_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    revoked_by TEXT,
    UNIQUE(tenant_id,id),
    FOREIGN KEY(tenant_id,application_id) REFERENCES service_application(tenant_id,application_id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,owner_id) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,created_by) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,revoked_by) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT,
    CHECK((revoked_at IS NULL AND revoked_by IS NULL) OR (revoked_at IS NOT NULL AND revoked_by IS NOT NULL))
);
CREATE INDEX IF NOT EXISTS idx_service_credential_app ON service_credential(tenant_id,application_id,created_at DESC,id);
CREATE TABLE IF NOT EXISTS service_credential_receipt (
    tenant_id TEXT NOT NULL,
    application_id TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    action TEXT NOT NULL CHECK(action IN ('issue_credential','revoke_credential')),
    semantics TEXT NOT NULL CHECK(jsonb_typeof(semantics::jsonb) IS NOT NULL),
    result TEXT NOT NULL CHECK(jsonb_typeof(result::jsonb) IS NOT NULL),
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY(tenant_id,application_id,idempotency_key),
    FOREIGN KEY(tenant_id,application_id) REFERENCES service_application(tenant_id,application_id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,actor_id) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS service_subscription (
    tenant_id TEXT NOT NULL,
    subscription_id TEXT NOT NULL,
    resource_kind TEXT NOT NULL DEFAULT 'service_subscription' CHECK(resource_kind='service_subscription'),
    resource_version BIGINT NOT NULL CHECK(resource_version>0),
    application_id TEXT NOT NULL,
    owner_id TEXT NOT NULL,
    requester_id TEXT NOT NULL,
    api_kind TEXT NOT NULL DEFAULT 'data_api' CHECK(api_kind='data_api'),
    api_id TEXT NOT NULL,
    api_version BIGINT NOT NULL CHECK(api_version>0),
    status TEXT NOT NULL CHECK(status IN ('draft','pending','approved','rejected','revoked')),
    purpose TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    approved_by TEXT,
    approved_at TIMESTAMPTZ,
    revoked_by TEXT,
    revoked_at TIMESTAMPTZ,
    PRIMARY KEY(tenant_id,subscription_id),
    FOREIGN KEY(tenant_id,resource_kind,subscription_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,application_id) REFERENCES service_application(tenant_id,application_id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,api_kind,api_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,owner_id) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,requester_id) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,approved_by) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,revoked_by) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT,
    CHECK((approved_by IS NULL AND approved_at IS NULL) OR (approved_by IS NOT NULL AND approved_at IS NOT NULL)),
    CHECK((revoked_by IS NULL AND revoked_at IS NULL) OR (revoked_by IS NOT NULL AND revoked_at IS NOT NULL))
);
CREATE INDEX IF NOT EXISTS idx_service_subscription_owner ON service_subscription(tenant_id,owner_id,status,subscription_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_service_subscription_active ON service_subscription(tenant_id,application_id,api_id) WHERE status='approved';
CREATE TABLE IF NOT EXISTS service_subscription_decision (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    subscription_id TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    action TEXT NOT NULL CHECK(action IN ('submit','approve','reject','revoke')),
    idempotency_key TEXT NOT NULL,
    semantics TEXT NOT NULL CHECK(jsonb_typeof(semantics::jsonb) IS NOT NULL),
    result TEXT NOT NULL CHECK(jsonb_typeof(result::jsonb) IS NOT NULL),
    created_at TIMESTAMPTZ NOT NULL,
    UNIQUE(tenant_id,subscription_id,idempotency_key),
    FOREIGN KEY(tenant_id,subscription_id) REFERENCES service_subscription(tenant_id,subscription_id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,actor_id) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS service_api_minute_quota (
    tenant_id TEXT NOT NULL,
    api_kind TEXT NOT NULL DEFAULT 'data_api' CHECK(api_kind='data_api'),
    api_id TEXT NOT NULL,
    minute_bucket TIMESTAMPTZ NOT NULL,
    reserved_count BIGINT NOT NULL CHECK(reserved_count>0),
    PRIMARY KEY(tenant_id,api_id,minute_bucket),
    FOREIGN KEY(tenant_id,api_kind,api_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT
);
CREATE TABLE IF NOT EXISTS service_credential_minute_quota (
    tenant_id TEXT NOT NULL,
    credential_id TEXT NOT NULL,
    api_kind TEXT NOT NULL DEFAULT 'data_api' CHECK(api_kind='data_api'),
    api_id TEXT NOT NULL,
    minute_bucket TIMESTAMPTZ NOT NULL,
    reserved_count BIGINT NOT NULL CHECK(reserved_count>0),
    PRIMARY KEY(tenant_id,credential_id,api_id,minute_bucket),
    FOREIGN KEY(tenant_id,credential_id) REFERENCES service_credential(tenant_id,id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,api_kind,api_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT
);
CREATE TABLE IF NOT EXISTS service_call (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    owner_id TEXT NOT NULL,
    application_id TEXT NOT NULL,
    application_version BIGINT NOT NULL CHECK(application_version>0),
    credential_id TEXT NOT NULL,
    credential_version BIGINT NOT NULL CHECK(credential_version>0),
    subscription_id TEXT NOT NULL,
    subscription_version BIGINT NOT NULL CHECK(subscription_version>0),
    api_kind TEXT NOT NULL DEFAULT 'data_api' CHECK(api_kind='data_api'),
    api_id TEXT NOT NULL,
    api_version BIGINT NOT NULL CHECK(api_version>0),
    minute_bucket TIMESTAMPTZ NOT NULL,
    rate_limit BIGINT NOT NULL CHECK(rate_limit BETWEEN 1 AND 100000),
    origin TEXT,
    outcome TEXT NOT NULL CHECK(outcome IN ('pending','succeeded','failed','denied','cancelled','reconciliation_required')),
    started_at TIMESTAMPTZ NOT NULL,
    finished_at TIMESTAMPTZ,
    duration_ms BIGINT CHECK(duration_ms BETWEEN 0 AND 86400000),
    row_count BIGINT CHECK(row_count BETWEEN 0 AND 10000),
    UNIQUE(tenant_id,id),
    FOREIGN KEY(tenant_id,application_id) REFERENCES service_application(tenant_id,application_id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,credential_id) REFERENCES service_credential(tenant_id,id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,subscription_id) REFERENCES service_subscription(tenant_id,subscription_id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,api_kind,api_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,owner_id) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT,
    CHECK((outcome='pending' AND finished_at IS NULL AND duration_ms IS NULL AND row_count IS NULL)
        OR (outcome='succeeded' AND finished_at IS NOT NULL AND duration_ms IS NOT NULL AND row_count IS NOT NULL)
        OR (outcome NOT IN ('pending','succeeded') AND finished_at IS NOT NULL AND duration_ms IS NOT NULL AND row_count IS NULL))
);
CREATE INDEX IF NOT EXISTS idx_service_call_owner ON service_call(tenant_id,owner_id,started_at DESC,id);
CREATE INDEX IF NOT EXISTS idx_service_call_app ON service_call(tenant_id,application_id,started_at DESC,id);

CREATE TABLE IF NOT EXISTS service_call_denial (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    owner_id TEXT NOT NULL,
    application_id TEXT NOT NULL,
    credential_id TEXT NOT NULL,
    api_id TEXT,
    api_kind TEXT NOT NULL DEFAULT 'data_api' CHECK(api_kind='data_api'),
    target_hash TEXT NOT NULL CHECK(length(target_hash)=64),
    reason_code TEXT NOT NULL CHECK(reason_code IN ('credential_unavailable','owner_inactive','application_inactive','origin_denied','api_unavailable','subscription_unavailable','rate_limited')),
    created_at TIMESTAMPTZ NOT NULL,
    FOREIGN KEY(tenant_id,application_id) REFERENCES service_application(tenant_id,application_id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,credential_id) REFERENCES service_credential(tenant_id,id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,api_kind,api_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,owner_id) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);
CREATE INDEX IF NOT EXISTS idx_service_call_denial_owner ON service_call_denial(tenant_id,owner_id,created_at DESC,id);
CREATE INDEX IF NOT EXISTS idx_service_call_denial_app ON service_call_denial(tenant_id,application_id,created_at DESC,id);

CREATE OR REPLACE FUNCTION service_access_receipt_immutable() RETURNS TRIGGER AS $$
BEGIN RAISE EXCEPTION 'service_access_receipt_immutable' USING ERRCODE='23514'; END;
$$ LANGUAGE plpgsql;
DROP TRIGGER IF EXISTS service_credential_receipt_immutable ON service_credential_receipt;
CREATE TRIGGER service_credential_receipt_immutable BEFORE UPDATE OR DELETE ON service_credential_receipt
FOR EACH ROW EXECUTE FUNCTION service_access_receipt_immutable();
DROP TRIGGER IF EXISTS service_subscription_decision_immutable ON service_subscription_decision;
CREATE TRIGGER service_subscription_decision_immutable BEFORE UPDATE OR DELETE ON service_subscription_decision
FOR EACH ROW EXECUTE FUNCTION service_access_receipt_immutable();
DROP TRIGGER IF EXISTS service_call_denial_immutable ON service_call_denial;
CREATE TRIGGER service_call_denial_immutable BEFORE UPDATE OR DELETE ON service_call_denial
FOR EACH ROW EXECUTE FUNCTION service_access_receipt_immutable();

CREATE OR REPLACE FUNCTION service_credential_immutable() RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP='DELETE' THEN RAISE EXCEPTION 'service_credential_immutable' USING ERRCODE='23514'; END IF;
    IF ROW(NEW.id,NEW.tenant_id,NEW.application_id,NEW.owner_id,NEW.verifier,NEW.created_by,NEW.created_at,NEW.expires_at)
        IS DISTINCT FROM ROW(OLD.id,OLD.tenant_id,OLD.application_id,OLD.owner_id,OLD.verifier,OLD.created_by,OLD.created_at,OLD.expires_at)
        OR OLD.revoked_at IS NOT NULL OR NEW.revoked_at IS NULL OR NEW.revoked_by IS NULL OR NEW.version<>OLD.version+1 THEN
        RAISE EXCEPTION 'service_credential_immutable' USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
DROP TRIGGER IF EXISTS service_credential_immutable ON service_credential;
CREATE TRIGGER service_credential_immutable BEFORE UPDATE OR DELETE ON service_credential
FOR EACH ROW EXECUTE FUNCTION service_credential_immutable();

CREATE OR REPLACE FUNCTION service_call_immutable() RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP='DELETE' THEN RAISE EXCEPTION 'service_call_immutable' USING ERRCODE='23514'; END IF;
    IF OLD.outcome<>'pending' OR NEW.outcome='pending' OR
        ROW(NEW.id,NEW.tenant_id,NEW.owner_id,NEW.application_id,NEW.application_version,NEW.credential_id,NEW.credential_version,
            NEW.subscription_id,NEW.subscription_version,NEW.api_kind,NEW.api_id,NEW.api_version,NEW.minute_bucket,NEW.rate_limit,NEW.origin,NEW.started_at)
        IS DISTINCT FROM ROW(OLD.id,OLD.tenant_id,OLD.owner_id,OLD.application_id,OLD.application_version,OLD.credential_id,OLD.credential_version,
            OLD.subscription_id,OLD.subscription_version,OLD.api_kind,OLD.api_id,OLD.api_version,OLD.minute_bucket,OLD.rate_limit,OLD.origin,OLD.started_at) THEN
        RAISE EXCEPTION 'service_call_immutable' USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
DROP TRIGGER IF EXISTS service_call_immutable ON service_call;
CREATE TRIGGER service_call_immutable BEFORE UPDATE OR DELETE ON service_call
FOR EACH ROW EXECUTE FUNCTION service_call_immutable();
