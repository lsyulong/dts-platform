CREATE UNIQUE INDEX IF NOT EXISTS idx_user_tenant_id_status
    ON "user"(tenant_id, id, status);
CREATE UNIQUE INDEX IF NOT EXISTS idx_role_tenant_id
    ON role(tenant_id, id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_datasource_tenant_id
    ON datasource(tenant_id, id);

CREATE TABLE IF NOT EXISTS product_resource_external_links (
    tenant_id TEXT NOT NULL,
    source_kind TEXT NOT NULL,
    source_id TEXT NOT NULL,
    field TEXT NOT NULL,
    target_kind TEXT NOT NULL,
    target_id TEXT NOT NULL,
    user_id TEXT,
    user_status TEXT,
    role_id TEXT,
    datasource_id TEXT,
    PRIMARY KEY (tenant_id, source_kind, source_id, field, target_kind, target_id),
    CHECK (
        (target_kind = 'user' AND user_id IS NOT NULL AND user_id = target_id AND user_status IS NOT NULL
         AND user_status <> 'deleted' AND role_id IS NULL AND datasource_id IS NULL)
        OR (target_kind = 'role' AND role_id IS NOT NULL AND role_id = target_id AND user_id IS NULL
            AND user_status IS NULL AND datasource_id IS NULL)
        OR (target_kind = 'datasource' AND datasource_id IS NOT NULL AND datasource_id = target_id AND user_id IS NULL
            AND user_status IS NULL AND role_id IS NULL)
    ),
    FOREIGN KEY (tenant_id, source_kind, source_id)
        REFERENCES product_resource(tenant_id, kind, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, user_id, user_status)
        REFERENCES "user"(tenant_id, id, status) ON UPDATE CASCADE ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, role_id)
        REFERENCES role(tenant_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, datasource_id)
        REFERENCES datasource(tenant_id, id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_product_external_target
    ON product_resource_external_links(tenant_id, target_kind, target_id);

CREATE TRIGGER IF NOT EXISTS product_external_user_insert
BEFORE INSERT ON product_resource_external_links
WHEN NEW.target_kind = 'user' AND NOT EXISTS (
    SELECT 1 FROM "user" WHERE tenant_id = NEW.tenant_id
    AND id = NEW.user_id AND status = 'active' AND NEW.user_status = 'active'
)
BEGIN
    SELECT RAISE(ABORT, 'product_reference_user_not_active');
END;

CREATE TRIGGER IF NOT EXISTS product_external_user_update
BEFORE UPDATE OF tenant_id, target_kind, target_id, user_id ON product_resource_external_links
WHEN NEW.target_kind = 'user' AND (
    NEW.tenant_id <> OLD.tenant_id OR NEW.target_kind <> OLD.target_kind
    OR NEW.target_id <> OLD.target_id OR NEW.user_id IS NOT OLD.user_id
) AND NOT EXISTS (
    SELECT 1 FROM "user" WHERE tenant_id = NEW.tenant_id
    AND id = NEW.user_id AND status = 'active' AND NEW.user_status = 'active'
)
BEGIN
    SELECT RAISE(ABORT, 'product_reference_user_not_active');
END;
