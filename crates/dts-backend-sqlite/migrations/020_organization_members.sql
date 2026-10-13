CREATE UNIQUE INDEX IF NOT EXISTS idx_organization_user_scope ON "user"(tenant_id,id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_organization_user_status ON "user"(tenant_id,id,status);
CREATE TABLE IF NOT EXISTS organization_membership (
    tenant_id TEXT NOT NULL, user_id TEXT NOT NULL, user_status TEXT NOT NULL,
    id TEXT NOT NULL UNIQUE, version BIGINT NOT NULL CHECK(version>0),
    organization_kind TEXT NOT NULL DEFAULT 'organization' CHECK(organization_kind='organization'),
    organization_id TEXT, changed_by TEXT NOT NULL, changed_at TEXT NOT NULL,
    PRIMARY KEY(tenant_id,user_id), UNIQUE(tenant_id,id), UNIQUE(tenant_id,organization_id,user_id),
    CHECK(organization_id IS NULL OR user_status<>'deleted'),
    FOREIGN KEY(tenant_id,user_id,user_status) REFERENCES "user"(tenant_id,id,status) ON UPDATE CASCADE ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,organization_kind,organization_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,changed_by) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);
CREATE TABLE IF NOT EXISTS organization_manager (
    tenant_id TEXT NOT NULL, organization_id TEXT NOT NULL, manager_user_id TEXT NOT NULL,
    manager_status TEXT NOT NULL CHECK(manager_status<>'deleted'),
    organization_kind TEXT NOT NULL DEFAULT 'organization' CHECK(organization_kind='organization'),
    PRIMARY KEY(tenant_id,organization_id),
    FOREIGN KEY(tenant_id,organization_kind,organization_id) REFERENCES product_resource(tenant_id,kind,id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,organization_id,manager_user_id) REFERENCES organization_membership(tenant_id,organization_id,user_id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,manager_user_id,manager_status) REFERENCES "user"(tenant_id,id,status) ON UPDATE CASCADE ON DELETE RESTRICT
);
CREATE TABLE IF NOT EXISTS organization_membership_receipt (
    id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, idempotency_key TEXT NOT NULL,
    action TEXT NOT NULL CHECK(action IN('transfer','remove','set_manager')),
    actor_id TEXT NOT NULL, user_id TEXT, source_organization_id TEXT, target_organization_id TEXT,
    reason TEXT NOT NULL, request_semantics TEXT NOT NULL CHECK(json_valid(request_semantics)),
    result_snapshot TEXT NOT NULL CHECK(json_valid(result_snapshot)), created_at TEXT NOT NULL,
    UNIQUE(tenant_id,idempotency_key),
    FOREIGN KEY(tenant_id,actor_id) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT,
    FOREIGN KEY(tenant_id,user_id) REFERENCES "user"(tenant_id,id) ON DELETE RESTRICT
);
CREATE INDEX IF NOT EXISTS idx_organization_members ON organization_membership(tenant_id,organization_id,user_id);
CREATE INDEX IF NOT EXISTS idx_organization_member_history ON organization_membership_receipt(tenant_id,created_at,id);
CREATE TRIGGER IF NOT EXISTS trg_organization_membership_version BEFORE UPDATE ON organization_membership
WHEN NEW.id<>OLD.id OR NEW.tenant_id<>OLD.tenant_id OR NEW.user_id<>OLD.user_id
 OR (NEW.organization_id IS NOT OLD.organization_id AND NEW.version<>OLD.version+1)
 OR (NEW.organization_id IS OLD.organization_id AND (NEW.version<>OLD.version OR NEW.changed_by<>OLD.changed_by OR NEW.changed_at<>OLD.changed_at))
BEGIN SELECT RAISE(ABORT,'organization_membership_version'); END;
CREATE TRIGGER IF NOT EXISTS trg_organization_membership_no_delete BEFORE DELETE ON organization_membership
BEGIN SELECT RAISE(ABORT,'organization_membership_history'); END;
CREATE TRIGGER IF NOT EXISTS trg_organization_receipt_no_update BEFORE UPDATE ON organization_membership_receipt
BEGIN SELECT RAISE(ABORT,'organization_membership_history'); END;
CREATE TRIGGER IF NOT EXISTS trg_organization_receipt_no_delete BEFORE DELETE ON organization_membership_receipt
BEGIN SELECT RAISE(ABORT,'organization_membership_history'); END;
CREATE TRIGGER IF NOT EXISTS trg_organization_definition_insert BEFORE INSERT ON product_resource
WHEN NEW.kind='organization' AND (json_type(NEW.properties,'$.member_ids') IS NOT NULL OR json_type(NEW.properties,'$.manager_user_id') IS NOT NULL)
BEGIN SELECT RAISE(ABORT,'organization_membership_requires_typed_api'); END;
CREATE TRIGGER IF NOT EXISTS trg_organization_definition_update BEFORE UPDATE OF properties,status ON product_resource
WHEN NEW.kind='organization' AND (
    json_type(NEW.properties,'$.member_ids') IS NOT NULL OR json_type(NEW.properties,'$.manager_user_id') IS NOT NULL
    OR (NEW.status='inactive' AND EXISTS(
      WITH RECURSIVE subtree(id) AS (
        SELECT NEW.id UNION SELECT p.id FROM product_resource p JOIN subtree s ON p.parent_id=s.id WHERE p.tenant_id=NEW.tenant_id AND p.kind='organization')
      SELECT 1 FROM organization_membership m JOIN subtree s ON m.organization_id=s.id WHERE m.tenant_id=NEW.tenant_id)))
BEGIN SELECT RAISE(ABORT,'organization_membership_requires_typed_api'); END;
