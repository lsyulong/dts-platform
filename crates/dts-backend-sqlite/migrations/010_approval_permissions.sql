INSERT OR IGNORE INTO role_permission(role_id,permission)
SELECT id,'master_data:approve' FROM role WHERE name='tenant_admin' AND is_system=1;
