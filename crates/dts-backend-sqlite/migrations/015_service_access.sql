CREATE UNIQUE INDEX IF NOT EXISTS idx_user_tenant_id ON "user"(tenant_id,id);
CREATE TABLE IF NOT EXISTS service_application (
    tenant_id TEXT NOT NULL,
    application_id TEXT NOT NULL,
    resource_kind TEXT NOT NULL DEFAULT 'service_application' CHECK(resource_kind='service_application'),
    resource_version INTEGER NOT NULL CHECK(resource_version>0),
    owner_id TEXT NOT NULL,
    owner_status TEXT NOT NULL,
    allowed_origins TEXT NOT NULL CHECK(json_valid(allowed_origins) AND json_type(allowed_origins)='array'),
    credential_expires_at TEXT NOT NULL,
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
    version INTEGER NOT NULL CHECK(version>0),
    verifier TEXT NOT NULL CHECK(length(verifier)=64),
    created_by TEXT NOT NULL,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    revoked_at TEXT,
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
    semantics TEXT NOT NULL CHECK(json_valid(semantics)),
    result TEXT NOT NULL CHECK(json_valid(result)),
    created_at TEXT NOT NULL,
    PRIMARY KEY(tenant_id,application_id,idempotency_key),
    FOREIGN KEY(tenant_id,application_id) REFERENCES service_application(tenant_id,application_id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,actor_id) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS service_subscription (
    tenant_id TEXT NOT NULL,
    subscription_id TEXT NOT NULL,
    resource_kind TEXT NOT NULL DEFAULT 'service_subscription' CHECK(resource_kind='service_subscription'),
    resource_version INTEGER NOT NULL CHECK(resource_version>0),
    application_id TEXT NOT NULL,
    owner_id TEXT NOT NULL,
    requester_id TEXT NOT NULL,
    api_kind TEXT NOT NULL DEFAULT 'data_api' CHECK(api_kind='data_api'),
    api_id TEXT NOT NULL,
    api_version INTEGER NOT NULL CHECK(api_version>0),
    status TEXT NOT NULL CHECK(status IN ('draft','pending','approved','rejected','revoked')),
    purpose TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    approved_by TEXT,
    approved_at TEXT,
    revoked_by TEXT,
    revoked_at TEXT,
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
    semantics TEXT NOT NULL CHECK(json_valid(semantics)),
    result TEXT NOT NULL CHECK(json_valid(result)),
    created_at TEXT NOT NULL,
    UNIQUE(tenant_id,subscription_id,idempotency_key),
    FOREIGN KEY(tenant_id,subscription_id) REFERENCES service_subscription(tenant_id,subscription_id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,actor_id) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS service_api_minute_quota (
    tenant_id TEXT NOT NULL,
    api_kind TEXT NOT NULL DEFAULT 'data_api' CHECK(api_kind='data_api'),
    api_id TEXT NOT NULL,
    minute_bucket TEXT NOT NULL,
    reserved_count INTEGER NOT NULL CHECK(reserved_count>0),
    PRIMARY KEY(tenant_id,api_id,minute_bucket),
    FOREIGN KEY(tenant_id,api_kind,api_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT
);
CREATE TABLE IF NOT EXISTS service_credential_minute_quota (
    tenant_id TEXT NOT NULL,
    credential_id TEXT NOT NULL,
    api_id TEXT NOT NULL,
    api_kind TEXT NOT NULL DEFAULT 'data_api' CHECK(api_kind='data_api'),
    minute_bucket TEXT NOT NULL,
    reserved_count INTEGER NOT NULL CHECK(reserved_count>0),
    PRIMARY KEY(tenant_id,credential_id,api_id,minute_bucket),
    FOREIGN KEY(tenant_id,credential_id) REFERENCES service_credential(tenant_id,id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,api_kind,api_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT
);
CREATE TABLE IF NOT EXISTS service_call (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    owner_id TEXT NOT NULL,
    application_id TEXT NOT NULL,
    application_version INTEGER NOT NULL CHECK(application_version>0),
    credential_id TEXT NOT NULL,
    credential_version INTEGER NOT NULL CHECK(credential_version>0),
    subscription_id TEXT NOT NULL,
    subscription_version INTEGER NOT NULL CHECK(subscription_version>0),
    api_id TEXT NOT NULL,
    api_kind TEXT NOT NULL DEFAULT 'data_api' CHECK(api_kind='data_api'),
    api_version INTEGER NOT NULL CHECK(api_version>0),
    minute_bucket TEXT NOT NULL,
    rate_limit INTEGER NOT NULL CHECK(rate_limit BETWEEN 1 AND 100000),
    origin TEXT,
    outcome TEXT NOT NULL CHECK(outcome IN ('pending','succeeded','failed','denied','cancelled','reconciliation_required')),
    started_at TEXT NOT NULL,
    finished_at TEXT,
    duration_ms INTEGER CHECK(duration_ms BETWEEN 0 AND 86400000),
    row_count INTEGER CHECK(row_count BETWEEN 0 AND 10000),
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
CREATE TRIGGER IF NOT EXISTS service_call_identity_immutable BEFORE UPDATE ON service_call
WHEN NEW.id<>OLD.id OR NEW.tenant_id<>OLD.tenant_id OR NEW.owner_id<>OLD.owner_id
 OR NEW.application_id<>OLD.application_id OR NEW.application_version<>OLD.application_version
 OR NEW.credential_id<>OLD.credential_id OR NEW.credential_version<>OLD.credential_version
 OR NEW.subscription_id<>OLD.subscription_id OR NEW.subscription_version<>OLD.subscription_version
 OR NEW.api_id<>OLD.api_id OR NEW.api_kind<>OLD.api_kind OR NEW.api_version<>OLD.api_version
 OR NEW.minute_bucket<>OLD.minute_bucket OR NEW.rate_limit<>OLD.rate_limit OR NEW.origin IS NOT OLD.origin
 OR NEW.started_at<>OLD.started_at OR OLD.outcome<>'pending' OR NEW.outcome='pending'
BEGIN SELECT RAISE(ABORT,'service_call_identity_immutable'); END;
CREATE TRIGGER IF NOT EXISTS service_call_no_delete BEFORE DELETE ON service_call
BEGIN SELECT RAISE(ABORT,'service_call_history_immutable'); END;

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
    created_at TEXT NOT NULL,
    FOREIGN KEY(tenant_id,application_id) REFERENCES service_application(tenant_id,application_id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,credential_id) REFERENCES service_credential(tenant_id,id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,api_kind,api_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,owner_id) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);
CREATE INDEX IF NOT EXISTS idx_service_call_denial_owner ON service_call_denial(tenant_id,owner_id,created_at DESC,id);
CREATE INDEX IF NOT EXISTS idx_service_call_denial_app ON service_call_denial(tenant_id,application_id,created_at DESC,id);
CREATE TRIGGER IF NOT EXISTS service_call_denial_no_update BEFORE UPDATE ON service_call_denial
BEGIN SELECT RAISE(ABORT,'service_call_denial_immutable'); END;
CREATE TRIGGER IF NOT EXISTS service_call_denial_no_delete BEFORE DELETE ON service_call_denial
BEGIN SELECT RAISE(ABORT,'service_call_denial_immutable'); END;

CREATE TRIGGER IF NOT EXISTS service_credential_immutable_identity BEFORE UPDATE ON service_credential
WHEN NEW.id<>OLD.id OR NEW.tenant_id<>OLD.tenant_id OR NEW.application_id<>OLD.application_id
 OR NEW.owner_id<>OLD.owner_id OR NEW.verifier<>OLD.verifier OR NEW.created_by<>OLD.created_by
 OR NEW.created_at<>OLD.created_at OR NEW.expires_at<>OLD.expires_at
 OR OLD.revoked_at IS NOT NULL OR NEW.version<>OLD.version+1 OR NEW.revoked_at IS NULL OR NEW.revoked_by IS NULL
BEGIN SELECT RAISE(ABORT,'service_credential_identity_immutable'); END;
CREATE TRIGGER IF NOT EXISTS service_credential_no_delete BEFORE DELETE ON service_credential
BEGIN SELECT RAISE(ABORT,'service_credential_history_immutable'); END;
CREATE TRIGGER IF NOT EXISTS service_credential_receipt_no_update BEFORE UPDATE ON service_credential_receipt
BEGIN SELECT RAISE(ABORT,'service_credential_receipt_immutable'); END;
CREATE TRIGGER IF NOT EXISTS service_credential_receipt_no_delete BEFORE DELETE ON service_credential_receipt
BEGIN SELECT RAISE(ABORT,'service_credential_receipt_immutable'); END;
CREATE TRIGGER IF NOT EXISTS service_subscription_decision_no_update BEFORE UPDATE ON service_subscription_decision
BEGIN SELECT RAISE(ABORT,'service_subscription_decision_immutable'); END;
CREATE TRIGGER IF NOT EXISTS service_subscription_decision_no_delete BEFORE DELETE ON service_subscription_decision
BEGIN SELECT RAISE(ABORT,'service_subscription_decision_immutable'); END;
