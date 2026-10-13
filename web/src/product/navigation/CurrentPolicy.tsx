import { useTranslation } from 'react-i18next';
import { Button } from '../../components/ui/button';
import { ErrorBanner } from '../../components/ui';
import { apiErrorMessage } from '../../lib/apiError';
import type { NavigationPolicyState } from './policy';

export default function CurrentNavigationPolicy({ state }: { state: NavigationPolicyState }) {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const policy = state.policy;
  const mode =
    policy?.mode === 'assigned'
      ? text('已分配模板', 'Assigned template')
      : policy?.mode === 'expired'
        ? text('策略已到期', 'Expired policy')
        : text('默认角色目录', 'Default role catalog');
  return (
    <section className="data-panel">
      <div className="data-toolbar">
        <h2 className="font-medium">{text('当前生效导航', 'Current navigation')}</h2>
        <Button
          variant="outline"
          disabled={state.phase === 'loading' || state.phase === 'unauthenticated'}
          onClick={() => void state.refresh()}
        >
          {text('刷新策略', 'Refresh policy')}
        </Button>
      </div>
      <div className="space-y-3 p-5">
        {state.phase === 'error' ? (
          <ErrorBanner
            message={apiErrorMessage(
              state.error,
              text(
                '导航策略加载失败，当前导航已收起',
                'Navigation failed to load; navigation is hidden',
              ),
            )}
          />
        ) : state.phase !== 'ready' ? (
          <p role="status">
            {state.phase === 'unauthenticated'
              ? text('请先登录', 'Sign in first')
              : text('正在加载导航策略…', 'Loading navigation policy…')}
          </p>
        ) : (
          policy && (
            <>
              <p>
                {mode} · {text('策略版本', 'Assignment version')} {policy.assignment_version}
              </p>
              {policy.template_id && (
                <p className="break-all text-sm">
                  {text('模板', 'Template')}: {policy.template_id} · v{policy.template_version}
                </p>
              )}
              <p className="text-sm">
                {text('到期时间', 'Expires at')}:{' '}
                {policy.expires_at
                  ? new Date(policy.expires_at).toLocaleString(i18n.language)
                  : text('无到期时间', 'No expiry')}
              </p>
              {policy.mode === 'expired' && (
                <p className="text-sm">
                  {text(
                    '到期后只保留有权限的恢复入口，不会自动展开全部目录。请重新分配或显式撤销。',
                    'Only permitted recovery routes remain after expiry. Reassign or explicitly revoke the policy to restore the catalog.',
                  )}
                </p>
              )}
              <p className="text-xs text-[var(--color-text-secondary)]">
                {text('可见入口', 'Visible routes')}: {policy.nodes.length} ·{' '}
                {text('过滤入口', 'Filtered routes')}: {policy.filtered_count}
              </p>
              {policy.nodes.length ? (
                <ul className="grid gap-2 sm:grid-cols-2">
                  {policy.nodes.map((node) => (
                    <li
                      key={node.id}
                      className="rounded-md bg-[var(--color-bg-secondary)] px-3 py-2 text-sm"
                    >
                      {node.name}{' '}
                      <span className="ml-2 text-xs text-[var(--color-text-tertiary)]">
                        {node.path}
                        {node.recovery ? ` · ${text('恢复入口', 'Recovery')}` : ''}
                      </span>
                    </li>
                  ))}
                </ul>
              ) : (
                <p className="text-sm">
                  {text('当前策略没有可见入口', 'No visible routes in the current policy')}
                </p>
              )}
            </>
          )
        )}
        <p className="text-xs text-[var(--color-text-tertiary)]">
          {text(
            '菜单可见范围与接口授权分别由当前角色权限校验；模板不会新增角色权限。',
            'Current role permissions govern route visibility and API authorization. Templates do not add role permissions.',
          )}
        </p>
      </div>
    </section>
  );
}
