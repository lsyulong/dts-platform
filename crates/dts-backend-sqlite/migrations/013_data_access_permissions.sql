INSERT OR IGNORE INTO role_permission(role_id,permission)
SELECT id,'data_security:approve' FROM role WHERE name='tenant_admin' AND is_system=1;
INSERT OR IGNORE INTO role_permission(role_id,permission)
SELECT id,'data_security:all_data' FROM role WHERE name='tenant_admin' AND is_system=1;
