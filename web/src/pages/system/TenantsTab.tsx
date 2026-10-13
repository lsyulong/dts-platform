import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import client from '../../api/client';
import { useQuery, useQueryClient, useMutation } from '@tanstack/react-query';
import { captureSession, isCurrentSession } from '../../lib/session';
import { toast } from 'sonner';
import { Edit2 } from 'lucide-react';
import { LoadingSpinner, EmptyState, StatusBadge, FormInput } from '../../components/ui';
import { Button } from '../../components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '../../components/ui/dialog';
import { useAuthStore } from '../../stores/auth';
import type { TenantItem } from './types';

export default function TenantsTab() {
  const { t } = useTranslation();
  const [editingTenant, setEditingTenant] = useState<TenantItem | null>(null);
  const currentTenantId = useAuthStore((state) => state.user?.tenant_id);

  const { data: tenants = [], isLoading } = useQuery({
    queryKey: ['system', 'tenants'],
    queryFn: () => client.get('/system/tenants').then((r) => r.data.data as TenantItem[]),
  });

  if (isLoading) return <LoadingSpinner />;
  const visibleTenants = tenants.filter((tenant) => tenant.id === currentTenantId);

  return (
    <div className="space-y-4">
      {visibleTenants.length === 0 ? (
        <EmptyState title={t('system.noTenants', 'No tenants')} />
      ) : (
        <div className="overflow-x-auto rounded-lg border border-[var(--color-border-default)]">
          <table className="data-table min-w-[680px]">
            <thead className="border-b border-[var(--color-border-default)] bg-[var(--color-bg-elevated)]">
              <tr>
                <th className="px-4 py-3 text-left text-[var(--color-text-secondary)] font-medium">
                  {t('system.tenantName', 'Name')}
                </th>
                <th className="px-4 py-3 text-left text-[var(--color-text-secondary)] font-medium">
                  {t('auth.tenantSlug', 'Slug')}
                </th>
                <th className="px-4 py-3 text-left text-[var(--color-text-secondary)] font-medium">
                  {t('system.status', 'Status')}
                </th>
                <th className="px-4 py-3 text-left text-[var(--color-text-secondary)] font-medium">
                  {t('system.createdAt', 'Created')}
                </th>
                <th className="px-4 py-3 text-right text-[var(--color-text-secondary)] font-medium">
                  {t('common.actions', 'Actions')}
                </th>
              </tr>
            </thead>
            <tbody className="divide-y divide-[var(--color-border-subtle)]">
              {visibleTenants.map((tenant) => (
                <tr key={tenant.id} className="hover:bg-[var(--color-bg-tertiary)]">
                  <td className="px-4 py-3 text-[var(--color-text-primary)] font-medium">
                    {tenant.name}
                  </td>
                  <td className="px-4 py-3 text-[var(--color-text-secondary)]">{tenant.slug}</td>
                  <td className="px-4 py-3">
                    <StatusBadge status={tenant.status} />
                  </td>
                  <td className="px-4 py-3 text-[var(--color-text-secondary)] text-xs">
                    {tenant.created_at}
                  </td>
                  <td className="px-4 py-3 text-right">
                    <button
                      onClick={() => setEditingTenant(tenant)}
                      className="rounded-lg p-1.5 text-[var(--color-text-tertiary)] hover:bg-[var(--color-bg-tertiary)] hover:text-[var(--color-text-primary)]"
                      title={t('common.edit', 'Edit')}
                    >
                      <Edit2 size={14} />
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {editingTenant && (
        <TenantEditDialog tenant={editingTenant} onClose={() => setEditingTenant(null)} />
      )}
    </div>
  );
}

function TenantEditDialog({ tenant, onClose }: { tenant: TenantItem; onClose: () => void }) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [form, setForm] = useState({ name: tenant.name, status: tenant.status });

  const updateMut = useMutation({
    onMutate: captureSession,
    mutationFn: (data: typeof form) => client.put(`/system/tenants/${tenant.id}`, data),
    onSuccess: (_data, _vars, session) => {
      if (!isCurrentSession(session)) return;
      qc.invalidateQueries({ queryKey: ['system', 'tenants'] });
      toast.success(t('system.tenantUpdated', 'Tenant updated'));
      onClose();
    },
    onError: (_error, _vars, session) => {
      if (isCurrentSession(session)) toast.error(t('errors.errorOccurred', 'Operation failed'));
    },
  });

  return (
    <Dialog open onOpenChange={onClose}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            {t('system.editTenant', 'Edit Tenant')} — {tenant.name}
          </DialogTitle>
        </DialogHeader>
        <div className="space-y-3 py-4">
          <FormInput
            label={t('system.tenantName', 'Name')}
            value={form.name}
            onChange={(v) => setForm({ ...form, name: v })}
            type="text"
          />
          <div>
            <label className="text-sm text-[var(--color-text-secondary)] mb-1 block">
              {t('system.status', 'Status')}
            </label>
            <select
              value={form.status}
              onChange={(e) => setForm({ ...form, status: e.target.value })}
              className="w-full rounded-lg border border-[var(--color-border-hover)] bg-[var(--color-bg-elevated)] px-3 py-2 text-sm text-[var(--color-text-primary)]"
            >
              <option value="active">active</option>
              <option value="disabled">disabled</option>
            </select>
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            {t('common.cancel', 'Cancel')}
          </Button>
          <Button onClick={() => updateMut.mutate(form)} disabled={updateMut.isPending}>
            {t('common.save', 'Save')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
