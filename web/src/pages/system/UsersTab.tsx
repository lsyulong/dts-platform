import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import client from '../../api/client';
import { useQuery, useQueryClient, useMutation } from '@tanstack/react-query';
import { toast } from 'sonner';
import { Plus, Trash2, Edit2, Key } from 'lucide-react';
import { LoadingSpinner, EmptyState, StatusBadge, FormInput } from '../../components/ui';
import { Button } from '../../components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '../../components/ui/dialog';
import type { RoleItem, UserItem } from './types';
import { apiErrorMessage } from '../../lib/apiError';
import { captureSession, isCurrentSession } from '../../lib/session';

export default function UsersTab() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [search, setSearch] = useState('');
  const [editingUser, setEditingUser] = useState<UserItem | null>(null);
  const [showCreate, setShowCreate] = useState(false);
  const [showResetPw, setShowResetPw] = useState<UserItem | null>(null);

  const { data: users = [], isLoading } = useQuery({
    queryKey: ['system', 'users'],
    queryFn: () => client.get('/system/users').then((r) => r.data.data as UserItem[]),
  });

  const deleteMut = useMutation({
    onMutate: captureSession,
    mutationFn: (id: string) => client.delete(`/system/users/${id}`),
    onSuccess: (_data, _vars, session) => {
      if (!isCurrentSession(session)) return;
      qc.invalidateQueries({ queryKey: ['system', 'users'] });
      toast.success(t('system.userDeleted', 'User deleted'));
    },
    onError: (_error, _vars, session) => {
      if (isCurrentSession(session)) toast.error(t('errors.errorOccurred', 'Operation failed'));
    },
  });

  const filtered = users.filter((u) => {
    if (!search) return true;
    const s = search.toLowerCase();
    return (
      u.username.toLowerCase().includes(s) ||
      u.email.toLowerCase().includes(s) ||
      (u.display_name || '').toLowerCase().includes(s)
    );
  });

  if (isLoading) return <LoadingSpinner />;

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between gap-4">
        <input
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder={t('system.searchUsers', 'Search users...')}
          className="flex-1 rounded-lg border border-[var(--color-border-hover)] bg-[var(--color-bg-elevated)] px-3 py-2 text-sm text-[var(--color-text-primary)] placeholder:text-[var(--color-text-tertiary)]"
        />
        <Button onClick={() => setShowCreate(true)}>
          <Plus size={14} className="mr-1" />
          {t('system.createUser', 'Create User')}
        </Button>
      </div>

      {filtered.length === 0 ? (
        <EmptyState title={t('system.noUsers', 'No users')} />
      ) : (
        <div className="overflow-x-auto rounded-lg border border-[var(--color-border-default)]">
          <table className="data-table min-w-[680px]">
            <thead className="border-b border-[var(--color-border-default)] bg-[var(--color-bg-elevated)]">
              <tr>
                <th className="px-4 py-3 text-left text-[var(--color-text-secondary)] font-medium">
                  {t('auth.username', 'Username')}
                </th>
                <th className="px-4 py-3 text-left text-[var(--color-text-secondary)] font-medium">
                  {t('auth.email', 'Email')}
                </th>
                <th className="px-4 py-3 text-left text-[var(--color-text-secondary)] font-medium">
                  {t('system.displayName', 'Display Name')}
                </th>
                <th className="px-4 py-3 text-left text-[var(--color-text-secondary)] font-medium">
                  {t('system.tenant', 'Tenant')}
                </th>
                <th className="px-4 py-3 text-left text-[var(--color-text-secondary)] font-medium">
                  {t('system.roles', 'Roles')}
                </th>
                <th className="px-4 py-3 text-left text-[var(--color-text-secondary)] font-medium">
                  {t('system.status', 'Status')}
                </th>
                <th className="px-4 py-3 text-right text-[var(--color-text-secondary)] font-medium">
                  {t('common.actions', 'Actions')}
                </th>
              </tr>
            </thead>
            <tbody className="divide-y divide-[var(--color-border-subtle)]">
              {filtered.map((u) => (
                <tr key={u.id} className="hover:bg-[var(--color-bg-tertiary)]">
                  <td className="px-4 py-3 text-[var(--color-text-primary)] font-medium">
                    {u.username}
                  </td>
                  <td className="px-4 py-3 text-[var(--color-text-secondary)]">{u.email}</td>
                  <td className="px-4 py-3 text-[var(--color-text-secondary)]">
                    {u.display_name || '-'}
                  </td>
                  <td className="px-4 py-3 text-[var(--color-text-secondary)]">{u.tenant_name}</td>
                  <td className="px-4 py-3">
                    <div className="flex flex-wrap gap-1">
                      {u.roles.map((r) => (
                        <span
                          key={r}
                          className="rounded-full bg-[var(--color-accent-subtle)] px-2 py-0.5 text-xs text-[var(--color-accent)]"
                        >
                          {r}
                        </span>
                      ))}
                    </div>
                  </td>
                  <td className="px-4 py-3">
                    <StatusBadge status={u.status} />
                  </td>
                  <td className="px-4 py-3 text-right">
                    <div className="flex items-center justify-end gap-1">
                      <button
                        onClick={() => setEditingUser(u)}
                        className="rounded-lg p-1.5 text-[var(--color-text-tertiary)] hover:bg-[var(--color-bg-tertiary)] hover:text-[var(--color-text-primary)]"
                        title={t('common.edit', 'Edit')}
                      >
                        <Edit2 size={14} />
                      </button>
                      <button
                        onClick={() => setShowResetPw(u)}
                        className="rounded-lg p-1.5 text-[var(--color-text-tertiary)] hover:bg-[var(--color-bg-tertiary)] hover:text-[var(--color-text-primary)]"
                        title={t('system.resetPassword', 'Reset Password')}
                      >
                        <Key size={14} />
                      </button>
                      <button
                        onClick={() => {
                          if (confirm(t('system.deleteConfirm', 'Delete this user?'))) {
                            deleteMut.mutate(u.id);
                          }
                        }}
                        className="rounded-lg p-1.5 text-[var(--color-text-tertiary)] hover:bg-red-900/40 hover:text-[var(--color-error)]"
                        title={t('common.delete', 'Delete')}
                      >
                        <Trash2 size={14} />
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {showCreate && <UserCreateDialog onClose={() => setShowCreate(false)} />}
      {editingUser && <UserEditDialog user={editingUser} onClose={() => setEditingUser(null)} />}
      {showResetPw && (
        <ResetPasswordDialog user={showResetPw} onClose={() => setShowResetPw(null)} />
      )}
    </div>
  );
}

function UserCreateDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [form, setForm] = useState({
    username: '',
    email: '',
    password: '',
    display_name: '',
    role_ids: '',
  });

  const createMut = useMutation({
    onMutate: captureSession,
    mutationFn: (data: typeof form) =>
      client.post('/system/users', {
        username: data.username,
        email: data.email,
        password: data.password,
        display_name: data.display_name || undefined,
        role_ids: data.role_ids
          ? data.role_ids
              .split(',')
              .map((s) => s.trim())
              .filter(Boolean)
          : [],
      }),
    onSuccess: (_data, _vars, session) => {
      if (!isCurrentSession(session)) return;
      qc.invalidateQueries({ queryKey: ['system', 'users'] });
      toast.success(t('system.userCreated', 'User created'));
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
          <DialogTitle>{t('system.createUser', 'Create User')}</DialogTitle>
          <DialogDescription>
            {t('system.createUserDesc', 'Fill in user info and assign roles')}
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-3 py-4">
          <FormInput
            label={t('auth.username', 'Username')}
            value={form.username}
            onChange={(v) => setForm({ ...form, username: v })}
            type="text"
          />
          <FormInput
            label={t('auth.email', 'Email')}
            value={form.email}
            onChange={(v) => setForm({ ...form, email: v })}
            type="email"
          />
          <FormInput
            label={t('auth.password', 'Password')}
            value={form.password}
            onChange={(v) => setForm({ ...form, password: v })}
            type="password"
          />
          <FormInput
            label={t('system.displayName', 'Display Name')}
            value={form.display_name}
            onChange={(v) => setForm({ ...form, display_name: v })}
            type="text"
          />
          <FormInput
            label={t('system.roles', 'Role IDs (comma separated)')}
            value={form.role_ids}
            onChange={(v) => setForm({ ...form, role_ids: v })}
            placeholder="role-id-1, role-id-2"
            type="text"
          />
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            {t('common.cancel', 'Cancel')}
          </Button>
          <Button onClick={() => createMut.mutate(form)} disabled={createMut.isPending}>
            {createMut.isPending ? t('common.loading', '...') : t('common.create', 'Create')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function UserEditDialog({ user, onClose }: { user: UserItem; onClose: () => void }) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [form, setForm] = useState<{
    display_name: string;
    role_ids: string | null;
    status: string;
  }>({
    display_name: user.display_name || '',
    role_ids: null,
    status: user.status,
  });
  const {
    data: roles = [],
    isPending,
    error,
    refetch,
  } = useQuery({
    queryKey: ['system', 'roles'],
    queryFn: () => client.get('/system/roles').then((r) => r.data.data as RoleItem[]),
  });
  const tenantRoles = roles.filter((role) => role.tenant_id === user.tenant_id);
  // UserListInfo contains role names, while the update endpoint requires role IDs.
  const matchedRoles = user.roles.map((name) => tenantRoles.filter((role) => role.name === name));
  const mappingFailed = matchedRoles.some((matches) => matches.length !== 1);
  const roleIdsText =
    form.role_ids ?? matchedRoles.map((matches) => matches[0]?.id ?? '').join(', ');
  const roleIds = [
    ...new Set(
      roleIdsText
        .split(',')
        .map((id) => id.trim())
        .filter(Boolean),
    ),
  ];
  const invalidIds = roleIds.some((id) => !tenantRoles.some((role) => role.id === id));
  const roleError = error
    ? apiErrorMessage(error, t('system.rolesLoadError'))
    : !isPending && mappingFailed
      ? t('system.roleMappingError')
      : !isPending && invalidIds
        ? t('system.roleIdsInvalid')
        : null;
  const canSave = !isPending && !roleError;

  const updateMut = useMutation({
    onMutate: captureSession,
    mutationFn: (data: { display_name: string; role_ids: string[]; status: string }) =>
      client.put(`/system/users/${user.id}`, {
        display_name: data.display_name || undefined,
        role_ids: data.role_ids,
        status: data.status,
      }),
    onSuccess: (_data, _vars, session) => {
      if (!isCurrentSession(session)) return;
      qc.invalidateQueries({ queryKey: ['system', 'users'] });
      toast.success(t('system.userUpdated', 'User updated'));
      onClose();
    },
    onError: (error, _vars, session) => {
      if (isCurrentSession(session)) toast.error(apiErrorMessage(error, t('errors.errorOccurred')));
    },
  });

  return (
    <Dialog open onOpenChange={onClose}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            {t('system.editUser', 'Edit User')} — {user.username}
          </DialogTitle>
        </DialogHeader>
        <div className="space-y-3 py-4">
          <FormInput
            label={t('system.displayName', 'Display Name')}
            value={form.display_name}
            onChange={(v) => setForm({ ...form, display_name: v })}
            type="text"
          />
          <FormInput
            label={t('system.roleIds')}
            value={roleIdsText}
            onChange={(v) => setForm({ ...form, role_ids: v })}
            placeholder="role-id-1, role-id-2"
            type="text"
            disabled={isPending || !!error || mappingFailed}
          />
          {isPending && <p role="status">{t('common.loading')}</p>}
          {roleError && (
            <p role="alert" className="text-sm text-[var(--color-error)]">
              {roleError}
            </p>
          )}
          {error && (
            <Button variant="outline" onClick={() => void refetch()}>
              {t('common.retry')}
            </Button>
          )}
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
          <Button
            onClick={() => {
              if (canSave && !updateMut.isPending) updateMut.mutate({ ...form, role_ids: roleIds });
            }}
            disabled={!canSave || updateMut.isPending}
          >
            {t('common.save', 'Save')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function ResetPasswordDialog({ user, onClose }: { user: UserItem; onClose: () => void }) {
  const { t } = useTranslation();
  const [newPassword, setNewPassword] = useState('');

  const resetMut = useMutation({
    onMutate: captureSession,
    mutationFn: (pw: string) =>
      client.post(`/system/users/${user.id}/reset-password`, { new_password: pw }),
    onSuccess: (_data, _vars, session) => {
      if (!isCurrentSession(session)) return;
      toast.success(t('system.passwordReset', 'Password reset'));
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
            {t('system.resetPassword', 'Reset Password')} — {user.username}
          </DialogTitle>
        </DialogHeader>
        <div className="py-4">
          <FormInput
            label={t('system.newPassword', 'New Password')}
            value={newPassword}
            onChange={(v) => setNewPassword(v)}
            type="password"
          />
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            {t('common.cancel', 'Cancel')}
          </Button>
          <Button onClick={() => resetMut.mutate(newPassword)} disabled={resetMut.isPending}>
            {t('common.confirm', 'Confirm')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
