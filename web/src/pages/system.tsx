import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  Shield, Users, Building2, UserCog, ShieldCheck,
  Plus, Trash2, Edit2, Key,
} from 'lucide-react';
import client from '../api/client';
import { useQuery, useQueryClient, useMutation } from '@tanstack/react-query';
import { LoadingSpinner, EmptyState, StatusBadge, FormInput } from '../components/ui';
import { Button } from '../components/ui/button';
import {
  Dialog, DialogContent, DialogHeader, DialogTitle,
  DialogDescription, DialogFooter,
} from '../components/ui/dialog';
import { toast } from 'sonner';

// ── types ────────────────────────────────────────────────────────────

interface UserItem {
  id: string;
  username: string;
  email: string;
  display_name?: string;
  tenant_id: string;
  tenant_name: string;
  roles: string[];
  status: string;
  created_at: string;
}

interface RoleItem {
  id: string;
  name: string;
  tenant_id: string;
  description: string;
  is_system: boolean;
  permissions: string[];
  created_at: string;
}

interface TenantItem {
  id: string;
  name: string;
  slug: string;
  status: string;
  created_at: string;
}

interface PermissionItem {
  code: string;
  group: string;
}

type Tab = 'users' | 'tenants' | 'roles' | 'permissions';

const PERMISSION_GROUPS: Record<string, string> = {
  task: 'Sync Task',
  ds: 'Data Source',
  alert: 'Alert',
  monitor: 'Monitor',
  ops: 'Ops',
  admin: 'Admin',
  system: 'System',
};

// ── page ─────────────────────────────────────────────────────────────

export default function SystemSettingsPage() {
  const { t } = useTranslation();
  const [activeTab, setActiveTab] = useState<Tab>('users');

  const tabs: { key: Tab; label: string; icon: React.ReactNode }[] = [
    { key: 'users', label: t('system.users', 'User Management'), icon: <Users size={14} /> },
    { key: 'tenants', label: t('system.tenants', 'Tenant Management'), icon: <Building2 size={14} /> },
    { key: 'roles', label: t('system.roles', 'Role Management'), icon: <UserCog size={14} /> },
    { key: 'permissions', label: t('system.permissionMatrix', 'Permission Matrix'), icon: <ShieldCheck size={14} /> },
  ];

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-xl font-bold text-[var(--color-text-primary)]">
          <Shield className="inline mr-2" size={22} />
          {t('nav.system', 'System Settings')}
        </h1>
        <p className="mt-1 text-sm text-[var(--color-text-tertiary)]">{t('system.subtitle', 'User, Role, Tenant & Permission Management')}</p>
      </div>

      <div className="flex items-center gap-2 border-b border-[var(--color-border-default)]">
        {tabs.map((tab) => (
          <button
            key={tab.key}
            onClick={() => setActiveTab(tab.key)}
            className={`flex items-center gap-2 border-b-2 px-4 py-3 text-sm font-medium transition-colors -mb-px ${
              activeTab === tab.key
                ? 'border-blue-500 text-blue-400'
                : 'border-transparent text-[var(--color-text-tertiary)] hover:text-[var(--color-text-secondary)]'
            }`}
          >
            {tab.icon}
            {tab.label}
          </button>
        ))}
      </div>

      {activeTab === 'users' && <UsersTab />}
      {activeTab === 'tenants' && <TenantsTab />}
      {activeTab === 'roles' && <RolesTab />}
      {activeTab === 'permissions' && <PermissionsTab />}
    </div>
  );
}

// ── Users Tab ────────────────────────────────────────────────────────

function UsersTab() {
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
    mutationFn: (id: string) => client.delete(`/system/users/${id}`),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['system', 'users'] });
      toast.success(t('system.userDeleted', 'User deleted'));
    },
    onError: () => toast.error(t('errors.errorOccurred', 'Operation failed')),
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
          className="flex-1 rounded-lg border border-[var(--color-border-hover)] bg-[var(--color-bg-tertiary)] px-3 py-2 text-sm text-[var(--color-text-primary)] placeholder:text-[var(--color-text-tertiary)]"
        />
        <Button onClick={() => setShowCreate(true)}>
          <Plus size={14} className="mr-1" />
          {t('system.createUser', 'Create User')}
        </Button>
      </div>

      {filtered.length === 0 ? (
        <EmptyState title={t('system.noUsers', 'No users')} />
      ) : (
        <div className="overflow-x-auto rounded-xl border border-[var(--color-border-default)]">
          <table className="w-full text-sm">
            <thead className="border-b border-[var(--color-border-default)] bg-[var(--color-bg-elevated)]">
              <tr>
                <th className="px-4 py-3 text-left text-[var(--color-text-secondary)] font-medium">{t('auth.username', 'Username')}</th>
                <th className="px-4 py-3 text-left text-[var(--color-text-secondary)] font-medium">{t('auth.email', 'Email')}</th>
                <th className="px-4 py-3 text-left text-[var(--color-text-secondary)] font-medium">{t('system.displayName', 'Display Name')}</th>
                <th className="px-4 py-3 text-left text-[var(--color-text-secondary)] font-medium">{t('system.tenant', 'Tenant')}</th>
                <th className="px-4 py-3 text-left text-[var(--color-text-secondary)] font-medium">{t('system.roles', 'Roles')}</th>
                <th className="px-4 py-3 text-left text-[var(--color-text-secondary)] font-medium">{t('system.status', 'Status')}</th>
                <th className="px-4 py-3 text-right text-[var(--color-text-secondary)] font-medium">{t('common.actions', 'Actions')}</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-[var(--color-border-subtle)]">
              {filtered.map((u) => (
                <tr key={u.id} className="hover:bg-[var(--color-bg-tertiary)]">
                  <td className="px-4 py-3 text-[var(--color-text-primary)] font-medium">{u.username}</td>
                  <td className="px-4 py-3 text-[var(--color-text-secondary)]">{u.email}</td>
                  <td className="px-4 py-3 text-[var(--color-text-secondary)]">{u.display_name || '-'}</td>
                  <td className="px-4 py-3 text-[var(--color-text-secondary)]">{u.tenant_name}</td>
                  <td className="px-4 py-3">
                    <div className="flex flex-wrap gap-1">
                      {u.roles.map((r) => (
                        <span key={r} className="rounded-full bg-blue-500/10 px-2 py-0.5 text-xs text-blue-400">
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
                        className="rounded-lg p-1.5 text-[var(--color-text-tertiary)] hover:bg-red-900/40 hover:text-red-400"
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
      {showResetPw && <ResetPasswordDialog user={showResetPw} onClose={() => setShowResetPw(null)} />}
    </div>
  );
}

function UserCreateDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [form, setForm] = useState({
    username: '', email: '', password: '', display_name: '', role_ids: '',
  });

  const createMut = useMutation({
    mutationFn: (data: typeof form) =>
      client.post('/system/users', {
        username: data.username,
        email: data.email,
        password: data.password,
        display_name: data.display_name || undefined,
        role_ids: data.role_ids ? data.role_ids.split(',').map((s) => s.trim()).filter(Boolean) : [],
      }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['system', 'users'] });
      toast.success(t('system.userCreated', 'User created'));
      onClose();
    },
    onError: () => toast.error(t('errors.errorOccurred', 'Operation failed')),
  });

  return (
    <Dialog open onOpenChange={onClose}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t('system.createUser', 'Create User')}</DialogTitle>
          <DialogDescription>{t('system.createUserDesc', 'Fill in user info and assign roles')}</DialogDescription>
        </DialogHeader>
        <div className="space-y-3 py-4">
          <FormInput label={t('auth.username', 'Username')} value={form.username} onChange={(v) => setForm({ ...form, username: v })} type="text" />
          <FormInput label={t('auth.email', 'Email')} value={form.email} onChange={(v) => setForm({ ...form, email: v })} type="email" />
          <FormInput label={t('auth.password', 'Password')} value={form.password} onChange={(v) => setForm({ ...form, password: v })} type="password" />
          <FormInput label={t('system.displayName', 'Display Name')} value={form.display_name} onChange={(v) => setForm({ ...form, display_name: v })} type="text" />
          <FormInput label={t('system.roles', 'Role IDs (comma separated)')} value={form.role_ids} onChange={(v) => setForm({ ...form, role_ids: v })} placeholder="role-id-1, role-id-2" type="text" />
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>{t('common.cancel', 'Cancel')}</Button>
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
  const [form, setForm] = useState({
    display_name: user.display_name || '',
    role_ids: user.roles.join(', '),
    status: user.status,
  });

  const updateMut = useMutation({
    mutationFn: (data: typeof form) =>
      client.put(`/system/users/${user.id}`, {
        display_name: data.display_name || undefined,
        role_ids: data.role_ids ? data.role_ids.split(',').map((s) => s.trim()).filter(Boolean) : [],
        status: data.status,
      }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['system', 'users'] });
      toast.success(t('system.userUpdated', 'User updated'));
      onClose();
    },
    onError: () => toast.error(t('errors.errorOccurred', 'Operation failed')),
  });

  return (
    <Dialog open onOpenChange={onClose}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t('system.editUser', 'Edit User')} — {user.username}</DialogTitle>
        </DialogHeader>
        <div className="space-y-3 py-4">
          <FormInput label={t('system.displayName', 'Display Name')} value={form.display_name} onChange={(v) => setForm({ ...form, display_name: v })} type="text" />
          <FormInput label={t('system.roles', 'Role IDs (comma separated)')} value={form.role_ids} onChange={(v) => setForm({ ...form, role_ids: v })} placeholder="role-id-1, role-id-2" type="text" />
          <div>
            <label className="text-sm text-[var(--color-text-secondary)] mb-1 block">{t('system.status', 'Status')}</label>
            <select
              value={form.status}
              onChange={(e) => setForm({ ...form, status: e.target.value })}
              className="w-full rounded-lg border border-[var(--color-border-hover)] bg-[var(--color-bg-tertiary)] px-3 py-2 text-sm text-[var(--color-text-primary)]"
            >
              <option value="active">active</option>
              <option value="disabled">disabled</option>
            </select>
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>{t('common.cancel', 'Cancel')}</Button>
          <Button onClick={() => updateMut.mutate(form)} disabled={updateMut.isPending}>
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
    mutationFn: (pw: string) => client.post(`/system/users/${user.id}/reset-password`, { new_password: pw }),
    onSuccess: () => {
      toast.success(t('system.passwordReset', 'Password reset'));
      onClose();
    },
    onError: () => toast.error(t('errors.errorOccurred', 'Operation failed')),
  });

  return (
    <Dialog open onOpenChange={onClose}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t('system.resetPassword', 'Reset Password')} — {user.username}</DialogTitle>
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
          <Button variant="outline" onClick={onClose}>{t('common.cancel', 'Cancel')}</Button>
          <Button onClick={() => resetMut.mutate(newPassword)} disabled={resetMut.isPending}>
            {t('common.confirm', 'Confirm')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

// ── Tenants Tab ──────────────────────────────────────────────────────

function TenantsTab() {
  const { t } = useTranslation();
  const [editingTenant, setEditingTenant] = useState<TenantItem | null>(null);

  const { data: tenants = [], isLoading } = useQuery({
    queryKey: ['system', 'tenants'],
    queryFn: () => client.get('/system/tenants').then((r) => r.data.data as TenantItem[]),
  });

  if (isLoading) return <LoadingSpinner />;

  return (
    <div className="space-y-4">
      {tenants.length === 0 ? (
        <EmptyState title={t('system.noTenants', 'No tenants')} />
      ) : (
        <div className="overflow-x-auto rounded-xl border border-[var(--color-border-default)]">
          <table className="w-full text-sm">
            <thead className="border-b border-[var(--color-border-default)] bg-[var(--color-bg-elevated)]">
              <tr>
                <th className="px-4 py-3 text-left text-[var(--color-text-secondary)] font-medium">{t('system.tenantName', 'Name')}</th>
                <th className="px-4 py-3 text-left text-[var(--color-text-secondary)] font-medium">{t('auth.tenantSlug', 'Slug')}</th>
                <th className="px-4 py-3 text-left text-[var(--color-text-secondary)] font-medium">{t('system.status', 'Status')}</th>
                <th className="px-4 py-3 text-left text-[var(--color-text-secondary)] font-medium">{t('system.createdAt', 'Created')}</th>
                <th className="px-4 py-3 text-right text-[var(--color-text-secondary)] font-medium">{t('common.actions', 'Actions')}</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-[var(--color-border-subtle)]">
              {tenants.map((tenant) => (
                <tr key={tenant.id} className="hover:bg-[var(--color-bg-tertiary)]">
                  <td className="px-4 py-3 text-[var(--color-text-primary)] font-medium">{tenant.name}</td>
                  <td className="px-4 py-3 text-[var(--color-text-secondary)]">{tenant.slug}</td>
                  <td className="px-4 py-3">
                    <StatusBadge status={tenant.status} />
                  </td>
                  <td className="px-4 py-3 text-[var(--color-text-secondary)] text-xs">{tenant.created_at}</td>
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

      {editingTenant && <TenantEditDialog tenant={editingTenant} onClose={() => setEditingTenant(null)} />}
    </div>
  );
}

function TenantEditDialog({ tenant, onClose }: { tenant: TenantItem; onClose: () => void }) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [form, setForm] = useState({ name: tenant.name, status: tenant.status });

  const updateMut = useMutation({
    mutationFn: (data: typeof form) => client.put(`/system/tenants/${tenant.id}`, data),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['system', 'tenants'] });
      toast.success(t('system.tenantUpdated', 'Tenant updated'));
      onClose();
    },
    onError: () => toast.error(t('errors.errorOccurred', 'Operation failed')),
  });

  return (
    <Dialog open onOpenChange={onClose}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t('system.editTenant', 'Edit Tenant')} — {tenant.name}</DialogTitle>
        </DialogHeader>
        <div className="space-y-3 py-4">
          <FormInput label={t('system.tenantName', 'Name')} value={form.name} onChange={(v) => setForm({ ...form, name: v })} type="text" />
          <div>
            <label className="text-sm text-[var(--color-text-secondary)] mb-1 block">{t('system.status', 'Status')}</label>
            <select
              value={form.status}
              onChange={(e) => setForm({ ...form, status: e.target.value })}
              className="w-full rounded-lg border border-[var(--color-border-hover)] bg-[var(--color-bg-tertiary)] px-3 py-2 text-sm text-[var(--color-text-primary)]"
            >
              <option value="active">active</option>
              <option value="disabled">disabled</option>
            </select>
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>{t('common.cancel', 'Cancel')}</Button>
          <Button onClick={() => updateMut.mutate(form)} disabled={updateMut.isPending}>
            {t('common.save', 'Save')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

// ── Roles Tab ────────────────────────────────────────────────────────

function RolesTab() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [showCreate, setShowCreate] = useState(false);
  const [editingRole, setEditingRole] = useState<RoleItem | null>(null);

  const { data: roles = [], isLoading } = useQuery({
    queryKey: ['system', 'roles'],
    queryFn: () => client.get('/system/roles').then((r) => r.data.data as RoleItem[]),
  });

  const deleteMut = useMutation({
    mutationFn: (id: string) => client.delete(`/system/roles/${id}`),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['system', 'roles'] });
      toast.success(t('system.roleDeleted', 'Role deleted'));
    },
    onError: () => toast.error(t('errors.errorOccurred', 'Operation failed')),
  });

  if (isLoading) return <LoadingSpinner />;

  return (
    <div className="space-y-4">
      <div className="flex justify-end">
        <Button onClick={() => setShowCreate(true)}>
          <Plus size={14} className="mr-1" />
          {t('system.createRole', 'Create Role')}
        </Button>
      </div>

      {roles.length === 0 ? (
        <EmptyState title={t('system.noRoles', 'No roles')} />
      ) : (
        <div className="space-y-3">
          {roles.map((r) => (
            <div key={r.id} className="rounded-xl border border-[var(--color-border-default)] bg-[var(--color-bg-secondary)] p-4">
              <div className="flex items-start justify-between">
                <div>
                  <div className="flex items-center gap-2">
                    <span className="font-medium text-[var(--color-text-primary)]">{r.name}</span>
                    {r.is_system && (
                      <span className="rounded-full bg-amber-500/10 px-2 py-0.5 text-xs text-amber-400">
                        {t('system.systemRole', 'System')}
                      </span>
                    )}
                  </div>
                  <p className="text-xs text-[var(--color-text-tertiary)] mt-1">{r.description}</p>
                </div>
                <div className="flex items-center gap-1">
                  <button
                    onClick={() => setEditingRole(r)}
                    className="rounded-lg p-1.5 text-[var(--color-text-tertiary)] hover:bg-[var(--color-bg-tertiary)] hover:text-[var(--color-text-primary)]"
                  >
                    <Edit2 size={14} />
                  </button>
                  {!r.is_system && (
                    <button
                      onClick={() => {
                        if (confirm(t('system.deleteConfirm', 'Confirm delete?'))) {
                          deleteMut.mutate(r.id);
                        }
                      }}
                      className="rounded-lg p-1.5 text-[var(--color-text-tertiary)] hover:bg-red-900/40 hover:text-red-400"
                    >
                      <Trash2 size={14} />
                    </button>
                  )}
                </div>
              </div>
              <div className="flex flex-wrap gap-1 mt-3">
                {r.permissions.map((p) => (
                  <span key={p} className="rounded-md bg-[var(--color-bg-tertiary)] px-1.5 py-0.5 text-xs text-[var(--color-text-secondary)] font-mono">
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
    mutationFn: (data: typeof form) => client.post('/system/roles', data),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['system', 'roles'] });
      toast.success(t('system.roleCreated', 'Role created'));
      onClose();
    },
    onError: () => toast.error(t('errors.errorOccurred', 'Operation failed')),
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
          <FormInput label={t('system.roleName', 'Name')} value={form.name} onChange={(v) => setForm({ ...form, name: v })} type="text" />
          <FormInput label={t('system.roleDesc', 'Description')} value={form.description} onChange={(v) => setForm({ ...form, description: v })} type="text" />
          <div>
            <label className="text-sm text-[var(--color-text-secondary)] mb-2 block">{t('system.permissions', 'Permissions')}</label>
            {Object.entries(grouped).map(([group, perms]) => (
              <div key={group} className="mb-3">
                <p className="text-xs font-medium text-[var(--color-text-tertiary)] uppercase mb-1">
                  {PERMISSION_GROUPS[group] || group}
                </p>
                <div className="space-y-1">
                  {perms.map((p) => (
                    <label key={p.code} className="flex items-center gap-2 text-sm text-[var(--color-text-secondary)] cursor-pointer">
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
          <Button variant="outline" onClick={onClose}>{t('common.cancel', 'Cancel')}</Button>
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
    mutationFn: (data: typeof form) =>
      client.put(`/system/roles/${role.id}`, {
        name: data.name || undefined,
        permissions: data.permissions,
      }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['system', 'roles'] });
      toast.success(t('system.roleUpdated', 'Role updated'));
      onClose();
    },
    onError: () => toast.error(t('errors.errorOccurred', 'Operation failed')),
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
          <DialogTitle>{t('system.editRole', 'Edit Role')} — {role.name}</DialogTitle>
        </DialogHeader>
        <div className="space-y-3 py-4 max-h-96 overflow-y-auto">
          <FormInput label={t('system.roleName', 'Name')} value={form.name} onChange={(v) => setForm({ ...form, name: v })} type="text" />
          <div>
            <label className="text-sm text-[var(--color-text-secondary)] mb-2 block">{t('system.permissions', 'Permissions')}</label>
            {Object.entries(grouped).map(([group, perms]) => (
              <div key={group} className="mb-3">
                <p className="text-xs font-medium text-[var(--color-text-tertiary)] uppercase mb-1">
                  {PERMISSION_GROUPS[group] || group}
                </p>
                <div className="space-y-1">
                  {perms.map((p) => (
                    <label key={p.code} className="flex items-center gap-2 text-sm text-[var(--color-text-secondary)] cursor-pointer">
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
          <Button variant="outline" onClick={onClose}>{t('common.cancel', 'Cancel')}</Button>
          <Button onClick={() => updateMut.mutate(form)} disabled={updateMut.isPending}>
            {t('common.save', 'Save')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

// ── Permissions Tab ──────────────────────────────────────────────────

function PermissionsTab() {
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
      <p className="text-sm text-[var(--color-text-tertiary)]">{t('system.permissionMatrixHint', 'Read-only view of role-to-permission mapping')}</p>

      <div className="overflow-x-auto rounded-xl border border-[var(--color-border-default)]">
        <table className="w-full text-sm">
          <thead className="border-b border-[var(--color-border-default)] bg-[var(--color-bg-elevated)]">
            <tr>
              <th className="px-4 py-3 text-left text-[var(--color-text-secondary)] font-medium">{t('system.permission', 'Permission')}</th>
              {roles.map((r) => (
                <th key={r.id} className="px-3 py-3 text-center text-[var(--color-text-secondary)] font-medium text-xs">
                  {r.name}
                </th>
              ))}
            </tr>
          </thead>
          <tbody className="divide-y divide-[var(--color-border-subtle)]">
            {perms.map((p) => (
              <tr key={p.code} className="hover:bg-[var(--color-bg-tertiary)]">
                <td className="px-4 py-2.5">
                  <span className="font-mono text-xs text-[var(--color-text-secondary)]">{p.code}</span>
                  <span className="ml-2 text-xs text-[var(--color-text-disabled)]">
                    ({PERMISSION_GROUPS[p.group] || p.group})
                  </span>
                </td>
                {roles.map((r) => (
                  <td key={r.id} className="px-3 py-2.5 text-center">
                    {r.permissions.includes(p.code) ? (
                      <span className="inline-block rounded-full bg-emerald-500/20 px-1.5 py-0.5 text-xs text-emerald-400">
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
