import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '../../components/ui/button';
import { ErrorBanner } from '../../components/ui';
import { apiErrorMessage } from '../../lib/apiError';
import type { ProductResource, ResourcePage } from '../../product/resource-types';
import { useNavigationQuery } from '../../product/navigation/api';
import { useNavigationSession } from '../../product/navigation/session';
import { useNavigationPolicy } from '../../product/navigation';
import { useNavigationOperations } from '../../product/navigation/operations';
import NavigationHistory from '../../product/navigation/History';
import NavigationOperationDialog from '../../product/navigation/OperationDialog';
import CurrentNavigationPolicy from '../../product/navigation/CurrentPolicy';

export interface NavigationPolicyPanelProps {
  className?: string;
  initialTemplateId?: string;
}

export default function NavigationPolicyPanel(props: NavigationPolicyPanelProps) {
  const session = useNavigationSession();
  return <NavigationPolicyPanelContent key={session.key} {...props} />;
}

function NavigationPolicyPanelContent({
  className = '',
  initialTemplateId = '',
}: NavigationPolicyPanelProps) {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const session = useNavigationSession();
  const state = useNavigationPolicy();
  const operations = useNavigationOperations(session);
  const [templateId, setTemplateId] = useState(initialTemplateId);
  const [offset, setOffset] = useState(0);
  const [q, setQ] = useState('');
  const canChoose = session.canManage && session.canReadDefinitions;
  const templates = useNavigationQuery<ResourcePage>(
    session,
    '/product/resources/menu_template',
    { status: 'draft', limit: 20, offset, q },
    canChoose,
  );
  const detail = useNavigationQuery<ProductResource>(
    session,
    `/product/resources/menu_template/${encodeURIComponent(templateId)}`,
    {},
    canChoose && !!templateId,
  );
  const template =
    !detail.isError &&
    !detail.isFetching &&
    detail.data?.tenant_id === session.tenantId &&
    detail.data.id === templateId &&
    detail.data.kind === 'menu_template' &&
    detail.data.status === 'draft'
      ? detail.data
      : null;
  const page = !templates.isError && !templates.isFetching ? templates.data : null;
  const assignment = state.policy;
  const validityDays =
    typeof template?.properties.validity_days === 'number'
      ? template.properties.validity_days
      : null;

  return (
    <div className={`space-y-5 ${className}`}>
      <CurrentNavigationPolicy state={state} />
      {session.ready && session.canManage && (
        <section className="data-panel p-5">
          <h2 className="mb-4 font-medium">
            {text('管理租户导航策略', 'Manage tenant navigation policy')}
          </h2>
          {!session.canReadDefinitions ? (
            <p>
              {text(
                '缺少 system:read 权限，无法读取真实模板定义',
                'system:read is required to read template definitions',
              )}
            </p>
          ) : (
            <>
              <div className="grid gap-4 sm:grid-cols-2">
                <div>
                  <label htmlFor="navigation-template-search" className="mb-1.5 block text-sm">
                    {text('搜索模板', 'Search templates')}
                  </label>
                  <input
                    id="navigation-template-search"
                    value={q}
                    maxLength={128}
                    disabled={operations.busy}
                    onChange={(event) => {
                      setQ(event.target.value);
                      setOffset(0);
                    }}
                    className="w-full rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] px-3 py-2 text-sm"
                  />
                </div>
                <div>
                  <label htmlFor="navigation-template" className="mb-1.5 block text-sm">
                    {text('真实菜单模板', 'Menu template')}
                  </label>
                  <select
                    id="navigation-template"
                    value={templateId}
                    disabled={operations.busy || templates.isFetching}
                    onChange={(event) => setTemplateId(event.target.value)}
                    className="w-full rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] px-3 py-2 text-sm"
                  >
                    <option value="">{text('请选择模板', 'Choose a template')}</option>
                    {templateId && !page?.data.some((item) => item.id === templateId) && (
                      <option value={templateId}>{template?.name ?? templateId}</option>
                    )}
                    {page?.data
                      .filter(
                        (item) => item.tenant_id === session.tenantId && item.status === 'draft',
                      )
                      .map((item) => (
                        <option key={item.id} value={item.id}>
                          {item.name} · v{item.version}
                        </option>
                      ))}
                  </select>
                </div>
              </div>
              <div className="mt-3 flex items-center justify-between gap-3 text-xs">
                <span>
                  {text('模板总数', 'Templates')}: {page?.total ?? '—'}
                </span>
                <div className="flex gap-2">
                  <Button
                    size="sm"
                    variant="outline"
                    disabled={!offset || templates.isFetching || operations.busy}
                    onClick={() => setOffset(Math.max(0, offset - 20))}
                  >
                    {text('模板上一页', 'Previous templates')}
                  </Button>
                  <Button
                    size="sm"
                    variant="outline"
                    disabled={
                      !page || offset + 20 >= page.total || templates.isFetching || operations.busy
                    }
                    onClick={() => setOffset(offset + 20)}
                  >
                    {text('模板下一页', 'Next templates')}
                  </Button>
                </div>
              </div>
              {(templates.error || detail.error) && (
                <div className="mt-3">
                  <ErrorBanner
                    message={apiErrorMessage(
                      templates.error ?? detail.error,
                      text('模板加载失败', 'Could not load templates'),
                    )}
                  />
                </div>
              )}
              {template && (
                <p className="mt-3 text-sm">
                  {text('选定版本', 'Selected version')}: v{template.version} ·{' '}
                  {validityDays === null
                    ? text('未设置模板有效期', 'No template validity period')
                    : text(
                        `模板有效期 ${validityDays} 天`,
                        `Template validity ${validityDays} days`,
                      )}
                </p>
              )}
            </>
          )}
          <div className="mt-4 flex flex-wrap gap-2">
            <Button
              disabled={!template || state.phase !== 'ready' || operations.busy}
              onClick={() => {
                if (template && assignment)
                  operations.begin({
                    kind: 'assign',
                    templateId: template.id,
                    templateName: template.name,
                    templateVersion: template.version,
                    assignmentVersion: assignment.assignment_version,
                    validityDays,
                    expiresAt: undefined,
                  });
              }}
            >
              {text('分配模板', 'Assign template')}
            </Button>
            <Button
              variant="outline"
              disabled={
                !assignment?.assignment_id ||
                assignment.mode === 'default_catalog' ||
                operations.busy
              }
              onClick={() => {
                if (assignment?.assignment_id)
                  operations.begin({
                    kind: 'revoke',
                    assignmentId: assignment.assignment_id,
                    assignmentVersion: assignment.assignment_version,
                    reason: '',
                  });
              }}
            >
              {text('撤销当前策略', 'Revoke current policy')}
            </Button>
          </div>
        </section>
      )}
      {operations.result && (
        <p role="status" className="data-panel p-4 text-sm">
          {operations.result.replayed
            ? text('已取得原请求的重试回执', 'Original request receipt replayed')
            : text('导航策略操作已提交', 'Navigation operation committed')}{' '}
          · {text('回执', 'Receipt')}: {operations.result.receipt_id} · v
          {operations.result.assignment.version}
        </p>
      )}
      {session.ready && <NavigationHistory session={session} />}
      <NavigationOperationDialog operations={operations} />
    </div>
  );
}
