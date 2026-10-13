import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '../../components/ui/button';
import { ErrorBanner } from '../../components/ui';
import { useAuthStore } from '../../stores/auth';
import { apiErrorMessage } from '../../lib/apiError';
import ProductModulePage from '../../product/ProductModulePage';
import type { ProductModule } from '../../product/catalog-types';
import { useDataAccessPage, type DataAccessGrant } from './data-access-api';

export default function MyDataAccessPage({ module }: { module: ProductModule }) {
  const identity = useAuthStore((state) =>
    JSON.stringify([state.user?.tenant_id, state.user?.id, state.token, state.user?.permissions]),
  );
  return <MyAccess key={identity} module={module} />;
}

function MyAccess({ module }: { module: ProductModule }) {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const canRead = !!useAuthStore((state) => state.user?.permissions.includes('data_security:read'));
  const [tab, setTab] = useState<'requests' | 'grants'>('requests');
  const grants = useDataAccessPage<{ data: DataAccessGrant[] }>(
    'grants',
    '',
    0,
    canRead && tab === 'grants',
  );
  return (
    <div className="space-y-5">
      <div
        className="flex gap-4 border-b border-[var(--color-border-default)]"
        role="tablist"
        aria-label={text('我的数据权限', 'My data access')}
      >
        {(['requests', 'grants'] as const).map((value) => (
          <button
            key={value}
            role="tab"
            aria-selected={tab === value}
            onClick={() => setTab(value)}
            className={`border-b-2 px-3 py-3 text-sm ${tab === value ? 'border-[var(--color-accent)] text-[var(--color-accent)]' : 'border-transparent text-[var(--color-text-secondary)]'}`}
          >
            {value === 'requests'
              ? text('我的申请', 'My requests')
              : text('有效授权', 'Active grants')}
          </button>
        ))}
      </div>
      {tab === 'requests' ? (
        <>
          <p className="text-sm text-[var(--color-text-tertiary)]">
            {text(
              '“已批准”表示审批决定已保存。当前可用权限请查看有效授权；到期、撤销或源结构变化都会影响查询权限。',
              'Approved means the decision was saved. See active grants for current access; expiry, revocation and source schema changes affect query permissions.',
            )}
          </p>
          <ProductModulePage module={module} />
        </>
      ) : (
        <>
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div>
              <h1 className="text-xl font-semibold">
                {text('有效数据授权', 'Active data grants')}
              </h1>
              <p className="mt-1 text-sm text-[var(--color-text-tertiary)]">
                {text(
                  '展示最近刷新时仍有效的表和字段权限；每次查询都会重新检查',
                  'Grants are valid at the last refresh; each query checks them again',
                )}
              </p>
            </div>
            <Button
              variant="outline"
              disabled={!canRead || grants.isFetching}
              onClick={() => void grants.refetch()}
            >
              {text('刷新授权', 'Refresh grants')}
            </Button>
          </div>
          {!canRead && (
            <ErrorBanner
              message={text('当前账号没有查看权限', 'Your account cannot read grants')}
            />
          )}
          {grants.error && (
            <ErrorBanner
              message={apiErrorMessage(grants.error, text('授权加载失败', 'Could not load grants'))}
            />
          )}
          <section className="data-panel">
            <div className="overflow-auto">
              <table className="w-full text-left text-sm">
                <thead className="bg-[var(--color-bg-secondary)]">
                  <tr>
                    {[
                      text('真实表', 'Table'),
                      text('允许字段', 'Allowed columns'),
                      text('来源申请', 'Source request'),
                      text('失效时间', 'Expires'),
                    ].map((label) => (
                      <th key={label} className="p-4 font-medium">
                        {label}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {canRead &&
                    grants.data?.data.map((grant) => (
                      <tr key={grant.id} className="border-t border-[var(--color-border-default)]">
                        <td className="p-4">
                          <strong>
                            {[grant.table_catalog, grant.schema_name, grant.table_name].join('.')}
                          </strong>
                          <p className="mt-1 text-xs text-[var(--color-text-tertiary)]">
                            {text('元数据版本', 'Metadata version')}: {grant.metadata_version}
                          </p>
                        </td>
                        <td className="max-w-96 break-words p-4">{grant.columns.join(', ')}</td>
                        <td className="p-4">
                          <p className="mt-1 text-xs text-[var(--color-text-tertiary)]">
                            {grant.request_id}
                          </p>
                        </td>
                        <td className="p-4">
                          {new Date(grant.expires_at).toLocaleString(i18n.language)}
                        </td>
                      </tr>
                    ))}
                </tbody>
              </table>
            </div>
            {canRead && !grants.data?.data.length && (
              <p className="py-14 text-center text-sm text-[var(--color-text-tertiary)]">
                {grants.isPending
                  ? text('正在检查有效授权', 'Checking active grants')
                  : text('当前没有有效的表字段授权', 'You have no active table grants')}
              </p>
            )}
          </section>
        </>
      )}
    </div>
  );
}
