import { useTranslation } from 'react-i18next';
import { useQuery } from '@tanstack/react-query';
import client from '../../api/client';
import { LoadingSpinner } from '../../components/ui';
import { PERMISSION_GROUPS, type RoleItem, type PermissionItem } from './types';

export default function PermissionsTab() {
  const { t } = useTranslation();

  const { data: roles = [], isLoading: rolesLoading } = useQuery({
    queryKey: ['system', 'roles'],
    queryFn: () => client.get('/system/roles').then((r) => r.data.data as RoleItem[]),
  });

  const { data: perms = [], isLoading: permsLoading } = useQuery({
    queryKey: ['system', 'permissions'],
    queryFn: () => client.get('/system/permissions').then((r) => r.data.data as PermissionItem[]),
  });

  if (rolesLoading || permsLoading) return <LoadingSpinner />;

  return (
    <div className="space-y-4">
      <p className="text-sm text-[var(--color-text-tertiary)]">
        {t('system.permissionMatrixHint', 'Read-only view of role-to-permission mapping')}
      </p>

      <div className="overflow-x-auto rounded-lg border border-[var(--color-border-default)]">
        <table className="data-table min-w-[680px]">
          <thead className="border-b border-[var(--color-border-default)] bg-[var(--color-bg-elevated)]">
            <tr>
              <th className="px-4 py-3 text-left text-[var(--color-text-secondary)] font-medium">
                {t('system.permission', 'Permission')}
              </th>
              {roles.map((r) => (
                <th
                  key={r.id}
                  className="px-3 py-3 text-center text-[var(--color-text-secondary)] font-medium text-xs"
                >
                  {r.name}
                </th>
              ))}
            </tr>
          </thead>
          <tbody className="divide-y divide-[var(--color-border-subtle)]">
            {perms.map((p) => (
              <tr key={p.code} className="hover:bg-[var(--color-bg-tertiary)]">
                <td className="px-4 py-2.5">
                  <span className="font-mono text-xs text-[var(--color-text-secondary)]">
                    {p.code}
                  </span>
                  <span className="ml-2 text-xs text-[var(--color-text-disabled)]">
                    ({PERMISSION_GROUPS[p.group] || p.group})
                  </span>
                </td>
                {roles.map((r) => (
                  <td key={r.id} className="px-3 py-2.5 text-center">
                    {r.permissions.includes(p.code) ? (
                      <span className="inline-block rounded-full bg-emerald-500/20 px-1.5 py-0.5 text-xs text-[var(--color-success)]">
                        ✓
                      </span>
                    ) : (
                      <span className="text-[var(--color-text-disabled)]">-</span>
                    )}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
