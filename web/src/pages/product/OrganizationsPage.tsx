import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { ErrorBanner } from '../../components/ui';
import ProductModulePage from '../../product/ProductModulePage';
import type { ProductModule } from '../../product/catalog-types';
import { useOrganizationSession } from '../../product/organization-members/api';
import OrganizationReferencePicker from '../../product/organization-members/Picker';
import OrganizationMembersPanel from './OrganizationMembersPanel';

export default function OrganizationsPage({ module }: { module: ProductModule }) {
  const session = useOrganizationSession();
  return <Organizations key={session.key} module={module} />;
}
function Organizations({ module }: { module: ProductModule }) {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const session = useOrganizationSession();
  const [organizationId, setOrganizationId] = useState('');
  if (!session.ready || !session.canReadDefinitions)
    return (
      <ErrorBanner
        message={text(
          '需要当前有效身份和组织查看权限',
          'An active identity and organization read permission are required',
        )}
      />
    );
  return (
    <div className="space-y-6">
      <ProductModulePage module={module} />
      <section className="data-panel p-5">
        <OrganizationReferencePicker
          kind="organization"
          label={text('选择组织查看成员和负责人', 'Organization for members and manager')}
          value={organizationId}
          onChange={setOrganizationId}
        />
      </section>
      {organizationId ? (
        <OrganizationMembersPanel organizationId={organizationId} />
      ) : (
        <p className="data-panel p-5 text-sm">
          {text(
            '选择组织后可查看实际成员及变更历史',
            'Select an organization to view current members and history',
          )}
        </p>
      )}
    </div>
  );
}
