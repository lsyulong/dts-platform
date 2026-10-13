CREATE UNIQUE INDEX IF NOT EXISTS idx_navigation_user_scope ON "user"(tenant_id,id);
CREATE TABLE IF NOT EXISTS tenant_navigation_assignment (
    tenant_id TEXT PRIMARY KEY REFERENCES tenant(id) ON DELETE RESTRICT,
    id TEXT NOT NULL UNIQUE,
    version BIGINT NOT NULL CHECK(version>0),
    status TEXT NOT NULL CHECK(status IN ('active','revoked')),
    template_kind TEXT NOT NULL DEFAULT 'menu_template' CHECK(template_kind='menu_template'),
    template_id TEXT NOT NULL,
    template_version BIGINT NOT NULL CHECK(template_version>0),
    template_snapshot TEXT NOT NULL CHECK(json_valid(template_snapshot) AND json_type(template_snapshot)='object'),
    assigned_by TEXT NOT NULL,
    assigned_at TEXT NOT NULL,
    expires_at TEXT,
    revoked_by TEXT,
    revoked_at TEXT,
    UNIQUE(tenant_id,id),
    CHECK((status='revoked')=(revoked_by IS NOT NULL)),
    CHECK((status='revoked')=(revoked_at IS NOT NULL)),
    CHECK(expires_at IS NULL OR expires_at>assigned_at),
    FOREIGN KEY(tenant_id,template_kind,template_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,assigned_by) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,revoked_by) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);
CREATE TABLE IF NOT EXISTS tenant_navigation_receipt (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    assignment_id TEXT NOT NULL,
    assignment_version BIGINT NOT NULL CHECK(assignment_version>0),
    idempotency_key TEXT NOT NULL,
    action TEXT NOT NULL CHECK(action IN ('assign','revoke')),
    actor_id TEXT NOT NULL,
    template_kind TEXT NOT NULL DEFAULT 'menu_template' CHECK(template_kind='menu_template'),
    template_id TEXT NOT NULL,
    request_semantics TEXT NOT NULL CHECK(json_valid(request_semantics) AND json_type(request_semantics)='object'),
    result_snapshot TEXT NOT NULL CHECK(json_valid(result_snapshot) AND json_type(result_snapshot)='object'),
    created_at TEXT NOT NULL,
    UNIQUE(tenant_id,idempotency_key),
    UNIQUE(tenant_id,assignment_id,assignment_version),
    UNIQUE(tenant_id,id),
    FOREIGN KEY(tenant_id,assignment_id) REFERENCES tenant_navigation_assignment(tenant_id,id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,actor_id) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,template_kind,template_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT
);
CREATE TABLE IF NOT EXISTS tenant_navigation_frozen_menu (
    tenant_id TEXT NOT NULL,
    receipt_id TEXT NOT NULL,
    menu_id TEXT NOT NULL,
    menu_kind TEXT NOT NULL DEFAULT 'system_menu' CHECK(menu_kind='system_menu'),
    menu_version BIGINT NOT NULL CHECK(menu_version>0),
    PRIMARY KEY(tenant_id,receipt_id,menu_id),
    FOREIGN KEY(tenant_id,receipt_id) REFERENCES tenant_navigation_receipt(tenant_id,id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,menu_kind,menu_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT
);
CREATE INDEX IF NOT EXISTS idx_navigation_history ON tenant_navigation_receipt(tenant_id,created_at,id);
CREATE TRIGGER IF NOT EXISTS trg_navigation_assignment_version BEFORE UPDATE ON tenant_navigation_assignment
WHEN NEW.tenant_id<>OLD.tenant_id OR NEW.id<>OLD.id OR NEW.version<>OLD.version+1
BEGIN SELECT RAISE(ABORT,'navigation_assignment_version'); END;
CREATE TRIGGER IF NOT EXISTS trg_navigation_assignment_no_delete BEFORE DELETE ON tenant_navigation_assignment
BEGIN SELECT RAISE(ABORT,'navigation_history_immutable'); END;
CREATE TRIGGER IF NOT EXISTS trg_navigation_receipt_no_update BEFORE UPDATE ON tenant_navigation_receipt
BEGIN SELECT RAISE(ABORT,'navigation_history_immutable'); END;
CREATE TRIGGER IF NOT EXISTS trg_navigation_receipt_no_delete BEFORE DELETE ON tenant_navigation_receipt
BEGIN SELECT RAISE(ABORT,'navigation_history_immutable'); END;
CREATE TRIGGER IF NOT EXISTS trg_navigation_menu_history_no_update BEFORE UPDATE ON tenant_navigation_frozen_menu
BEGIN SELECT RAISE(ABORT,'navigation_history_immutable'); END;
CREATE TRIGGER IF NOT EXISTS trg_navigation_menu_history_no_delete BEFORE DELETE ON tenant_navigation_frozen_menu
BEGIN SELECT RAISE(ABORT,'navigation_history_immutable'); END;

