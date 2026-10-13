import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import client from '../../api/client';
import { useQuery, useQueryClient, useMutation } from '@tanstack/react-query';
import { toast } from 'sonner';
import { Plus, Trash2, Edit2, ShieldCheck } from 'lucide-react';
import { LoadingSpinner, EmptyState, FormInput } from '../../components/ui';
import { Button } from '../../components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '../../components/ui/dialog';
import { PERMISSION_GROUPS, type RoleItem, type PermissionItem } from './types';
import { captureSession, isCurrentSession } from '../../lib/session';

export default function RolesTab() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [showCreate, setShowCreate] = useState(false);
  const [editingRole, setEditingRole] = useState<RoleItem | null>(null);

  const { data: roles = [], isLoading } = useQuery({
    queryKey: ['system', 'roles'],
    queryFn: () => client.get('/system/roles').then((r) => r.data.data as RoleItem[]),
  });

  const deleteMut = useMutation({
    onMutate: captureSession,
    mutationFn: (id: string) => client.delete(`/system/roles/${id}`),
    onSuccess: (_data, _vars, session) => {
      if (!isCurrentSession(session)) return;
      qc.invalidateQueries({ queryKey: ['system', 'roles'] });
      toast.success(t('system.roleDeleted', 'Role deleted'));
    },
    onError: (_error, _vars, session) => {
      if (isCurrentSession(session)) toast.error(t('errors.errorOccurred', 'Operation failed'));
    },
  });

  if (isLoading) return <LoadingSpinner />;

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between gap-3">
        <p className="text-sm text-[var(--color-text-secondary)]">
          {t('system.roleCount', { count: roles.length })}
        </p>
        <Button onClick={() => setShowCreate(true)}>
          <Plus size={14} className="mr-1" />
          {t('system.createRole', 'Create Role')}
        </Button>
      </div>

      {roles.length === 0 ? (
        <EmptyState title={t('system.noRoles', 'No roles')} />
      ) : (
        <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
          {roles.map((r) => (
            <div
              key={r.id}
              className="flex min-w-0 flex-col rounded-lg border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-4"
            >
              <div className="flex items-start justify-between">
                <div className="min-w-0">
                  <div className="flex items-center gap-2">
                    <ShieldCheck size={20} className="shrink-0 text-[var(--color-accent)]" />
                    <span
                      className="truncate font-medium text-[var(--color-text-primary)]"
                      title={r.name}
                    >
                      {r.name}
                    </span>
                    {r.is_system && (
                      <span className="shrink-0 rounded-full bg-[var(--color-accent-subtle)] px-2 py-0.5 text-xs text-[var(--color-accent)]">
                        {t('system.systemRole', 'System')}
                      </span>
                    )}
                  </div>
                  <p className="mt-2 min-h-8 break-words text-xs leading-5 text-[var(--color-text-tertiary)]">
                    {r.description || '—'}
                  </p>
                </div>
                <div className="flex items-center gap-1">
                  {!r.is_system && (
                    <button
                      onClick={() => setEditingRole(r)}
                      title={t('common.edit')}
                      className="rounded-lg p-1.5 text-[var(--color-text-tertiary)] hover:bg-[var(--color-bg-tertiary)] hover:text-[var(--color-text-primary)]"
                    >
                      <Edit2 size={14} />
                    </button>
                  )}
                  {!r.is_system && (
                    <button
                      onClick={() => {
                        if (confirm(t('system.deleteConfirm', 'Confirm delete?'))) {
                          deleteMut.mutate(r.id);
                        }
                      }}
                      title={t('common.delete')}
                      className="rounded-lg p-1.5 text-[var(--color-text-tertiary)] hover:bg-red-900/40 hover:text-[var(--color-error)]"
                    >
                      <Trash2 size={14} />
                    </button>
                  )}
                </div>
              </div>
              <p className="mt-4 border-t border-[var(--color-border-subtle)] pt-3 text-xs text-[var(--color-text-secondary)]">
                {t('system.permissionCount', { count: r.permissions.length })}
              </p>
              <div className="mt-2 flex flex-wrap gap-1">
                {r.permissions.map((p) => (
                  <span
                    key={p}
                    className="rounded-md bg-[var(--color-bg-tertiary)] px-1.5 py-0.5 text-xs text-[var(--color-text-secondary)] font-mono"
                  >
                    {p}
                  </span>
                ))}
              </div>
            </div>
          ))}
        </div>
      )}

      {showCreate && <RoleCreateDialog onClose={() => setShowCreate(false)} />}
      {editingRole && <RoleEditDialog role={editingRole} onClose={() => setEditingRole(null)} />}
    </div>
  );
}

function RoleCreateDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [form, setForm] = useState({ name: '', description: '', permissions: [] as string[] });

  const { data: allPerms = [] } = useQuery({
    queryKey: ['system', 'permissions'],
    queryFn: () => client.get('/system/permissions').then((r) => r.data.data as PermissionItem[]),
  });

  const createMut = useMutation({
    onMutate: captureSession,
    mutationFn: (data: typeof form) => client.post('/system/roles', data),
    onSuccess: (_data, _vars, session) => {
      if (!isCurrentSession(session)) return;
      qc.invalidateQueries({ queryKey: ['system', 'roles'] });
      toast.success(t('system.roleCreated', 'Role created'));
      onClose();
    },
    onError: (_error, _vars, session) => {
      if (isCurrentSession(session)) toast.error(t('errors.errorOccurred', 'Operation failed'));
    },
  });

  const togglePerm = (code: string) => {
    setForm((prev) => ({
      ...prev,
      permissions: prev.permissions.includes(code)
        ? prev.permissions.filter((p) => p !== code)
        : [...prev.permissions, code],
    }));
  };

  const grouped = allPerms.reduce<Record<string, PermissionItem[]>>((acc, p) => {
    const g = p.group || 'other';
    if (!acc[g]) acc[g] = [];
    acc[g].push(p);
    return acc;
  }, {});

  return (
    <Dialog open onOpenChange={onClose}>
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle>{t('system.createRole', 'Create Role')}</DialogTitle>
        </DialogHeader>
        <div className="space-y-3 py-4 max-h-96 overflow-y-auto">
          <FormInput
            label={t('system.roleName', 'Name')}
            value={form.name}
            onChange={(v) => setForm({ ...form, name: v })}
            type="text"
          />
          <FormInput
            label={t('system.roleDesc', 'Description')}
            value={form.description}
            onChange={(v) => setForm({ ...form, description: v })}
            type="text"
          />
          <div>
            <label className="text-sm text-[var(--color-text-secondary)] mb-2 block">
              {t('system.permissions', 'Permissions')}
            </label>
            {Object.entries(grouped).map(([group, perms]) => (
              <div key={group} className="mb-3">
                <p className="text-xs font-medium text-[var(--color-text-tertiary)] uppercase mb-1">
                  {PERMISSION_GROUPS[group] || group}
                </p>
                <div className="space-y-1">
                  {perms.map((p) => (
                    <label
                      key={p.code}
                      className="flex items-center gap-2 text-sm text-[var(--color-text-secondary)] cursor-pointer"
                    >
                      <input
                        type="checkbox"
                        checked={form.permissions.includes(p.code)}
                        onChange={() => togglePerm(p.code)}
                        className="rounded border-[var(--color-border-hover)] bg-[var(--color-bg-tertiary)]"
                      />
                      <span className="font-mono text-xs">{p.code}</span>
                    </label>
                  ))}
                </div>
              </div>
            ))}
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            {t('common.cancel', 'Cancel')}
          </Button>
          <Button onClick={() => createMut.mutate(form)} disabled={createMut.isPending}>
            {t('common.create', 'Create')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function RoleEditDialog({ role, onClose }: { role: RoleItem; onClose: () => void }) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [form, setForm] = useState({ name: role.name, permissions: role.permissions });

  const { data: allPerms = [] } = useQuery({
    queryKey: ['system', 'permissions'],
    queryFn: () => client.get('/system/permissions').then((r) => r.data.data as PermissionItem[]),
  });

  const updateMut = useMutation({
    onMutate: captureSession,
    mutationFn: (data: typeof form) =>
      client.put(`/system/roles/${role.id}`, {
        name: data.name || undefined,
        permissions: data.permissions,
      }),
    onSuccess: (_data, _vars, session) => {
      if (!isCurrentSession(session)) return;
      qc.invalidateQueries({ queryKey: ['system', 'roles'] });
      toast.success(t('system.roleUpdated', 'Role updated'));
      onClose();
    },
    onError: (_error, _vars, session) => {
      if (isCurrentSession(session)) toast.error(t('errors.errorOccurred', 'Operation failed'));
    },
  });

  const togglePerm = (code: string) => {
    setForm((prev) => ({
      ...prev,
      permissions: prev.permissions.includes(code)
        ? prev.permissions.filter((p) => p !== code)
        : [...prev.permissions, code],
    }));
  };

  const grouped = allPerms.reduce<Record<string, PermissionItem[]>>((acc, p) => {
    const g = p.group || 'other';
    if (!acc[g]) acc[g] = [];
    acc[g].push(p);
    return acc;
  }, {});

  return (
    <Dialog open onOpenChange={onClose}>
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle>
            {t('system.editRole', 'Edit Role')} — {role.name}
          </DialogTitle>
        </DialogHeader>
        <div className="space-y-3 py-4 max-h-96 overflow-y-auto">
          <FormInput
            label={t('system.roleName', 'Name')}
            value={form.name}
            onChange={(v) => setForm({ ...form, name: v })}
            type="text"
          />
          <div>
            <label className="text-sm text-[var(--color-text-secondary)] mb-2 block">
              {t('system.permissions', 'Permissions')}
            </label>
            {Object.entries(grouped).map(([group, perms]) => (
              <div key={group} className="mb-3">
                <p className="text-xs font-medium text-[var(--color-text-tertiary)] uppercase mb-1">
                  {PERMISSION_GROUPS[group] || group}
                </p>
                <div className="space-y-1">
                  {perms.map((p) => (
                    <label
                      key={p.code}
                      className="flex items-center gap-2 text-sm text-[var(--color-text-secondary)] cursor-pointer"
                    >
                      <input
                        type="checkbox"
                        checked={form.permissions.includes(p.code)}
                        onChange={() => togglePerm(p.code)}
                        className="rounded border-[var(--color-border-hover)] bg-[var(--color-bg-tertiary)]"
                      />
                      <span className="font-mono text-xs">{p.code}</span>
                    </label>
                  ))}
                </div>
              </div>
            ))}
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
