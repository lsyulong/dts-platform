import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Shield, Users, Building2, UserCog, ShieldCheck } from 'lucide-react';
import { ErrorBanner } from '../components/ui';
import { useAuthStore } from '../stores/auth';
import UsersTab from './system/UsersTab';
import TenantsTab from './system/TenantsTab';
import RolesTab from './system/RolesTab';
import PermissionsTab from './system/PermissionsTab';
import type { Tab } from './system/types';

export default function SystemSettingsPage({ initialTab = 'users' }: { initialTab?: Tab }) {
  const { t } = useTranslation();
  const [activeTab, setActiveTab] = useState<Tab>(initialTab);
  const user = useAuthStore((state) => state.user);

  const tabs: { key: Tab; label: string; icon: React.ReactNode }[] = [
    { key: 'users', label: t('system.users', 'User Management'), icon: <Users size={14} /> },
    {
      key: 'tenants',
      label: t('system.tenants', 'Tenant Management'),
      icon: <Building2 size={14} />,
    },
    { key: 'roles', label: t('system.roles', 'Role Management'), icon: <UserCog size={14} /> },
    {
      key: 'permissions',
      label: t('system.permissionMatrix', 'Permission Matrix'),
      icon: <ShieldCheck size={14} />,
    },
  ];

  if (!user?.permissions.includes('user:manage'))
    return <ErrorBanner message={t('errors.forbidden')} />;

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-xl font-bold text-[var(--color-text-primary)]">
          <Shield className="inline mr-2" size={22} />
          {t('nav.system', 'System Settings')}
        </h1>
        <p className="mt-1 text-sm text-[var(--color-text-tertiary)]">
          {t('system.subtitle', 'User, Role, Tenant & Permission Management')}
        </p>
      </div>

      <section className="data-panel">
        <div className="flex items-center gap-2 overflow-x-auto border-b border-[var(--color-border-default)] px-2">
          {tabs.map((tab) => (
            <button
              key={tab.key}
              onClick={() => setActiveTab(tab.key)}
              aria-pressed={activeTab === tab.key}
              className={`flex shrink-0 items-center gap-2 border-b-2 px-4 py-3 text-sm font-medium transition-colors -mb-px ${
                activeTab === tab.key
                  ? 'border-[var(--color-accent)] text-[var(--color-accent)]'
                  : 'border-transparent text-[var(--color-text-tertiary)] hover:text-[var(--color-text-secondary)]'
              }`}
            >
              {tab.icon}
              {tab.label}
            </button>
          ))}
        </div>

        <div className="p-4 sm:p-5">
          {activeTab === 'users' && <UsersTab />}
          {activeTab === 'tenants' && <TenantsTab />}
          {activeTab === 'roles' && <RolesTab />}
          {activeTab === 'permissions' && <PermissionsTab />}
        </div>
      </section>
    </div>
  );
}
