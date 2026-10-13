INSERT OR IGNORE INTO role_permission(role_id,permission)
SELECT id,'services:approve' FROM role WHERE name='tenant_admin' AND is_system=1;
