import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { Link } from 'react-router-dom';
import client from '../../api/client';
import { useAuthStore } from '../../stores/auth';
import { productModules } from '../../product/catalog';
import { localized, statusText } from '../../product/ui-text';
import type { PortalWidget } from '../../product/design-editors/design-models';
import { apiErrorMessage } from '../../lib/apiError';
import { ErrorBanner } from '../../components/ui';
import { useVisibleContent } from './system-content-api';
import { BarChart, Bar, XAxis, YAxis, Tooltip, ResponsiveContainer } from 'recharts';

function AnnouncementWidget() {
  const { i18n } = useTranslation();
  const content = useVisibleContent('announcement');
  if (content.error)
    return (
      <ErrorBanner
        message={apiErrorMessage(
          content.error,
          i18n.language.startsWith('zh') ? '公告读取失败' : 'Could not load announcements',
        )}
      />
    );
  if (content.isPending)
    return (
      <p className="text-xs">
        {i18n.language.startsWith('zh') ? '正在加载公告' : 'Loading announcements'}
      </p>
    );
  if (!content.data?.length)
    return (
      <p className="text-xs text-[var(--color-text-tertiary)]">
        {i18n.language.startsWith('zh') ? '暂无公告' : 'No announcements'}
      </p>
    );
  return (
    <ul className="space-y-3">
      {content.data?.slice(0, 5).map((item) => (
        <li key={item.id}>
          <strong className="text-sm">{item.name}</strong>
          <p className="mt-1 line-clamp-3 whitespace-pre-wrap text-xs text-[var(--color-text-tertiary)]">
            {String(item.properties.content ?? '')}
          </p>
        </li>
      ))}
    </ul>
  );
}

export default function PortalWidgetView({ widget }: { widget: PortalWidget }) {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const user = useAuthStore((state) => state.user);
  const token = useAuthStore((state) => state.token);
  const module = productModules.find((module) => module.id === widget.module_id);
  const domain = module?.groupId === 'access' ? 'system' : module?.groupId.replaceAll('-', '_');
  const permission = module?.permission ?? `${domain}:read`;
  const canRead = !!user?.permissions.includes(permission);
  const path =
    module?.path === '/tasks'
      ? '/tasks'
      : module?.path === '/datasources'
        ? '/datasources'
        : module?.resourceKind
          ? `/product/resources/${module.resourceKind}`
          : '';
  const data = useQuery({
    queryKey: ['portal-widget', user?.tenant_id, user?.id, path],
    queryFn: async ({ signal }) => {
      const response = await client.get<{
        data: { id: string; name: string; status?: string; health_status?: string }[];
        total?: number;
      }>(path, {
        params: { limit: 5, offset: 0 },
        signal,
        headers: { Authorization: `Bearer ${token}` },
      });
      return {
        rows: response.data.data.slice(0, 5),
        total: response.data.total ?? response.data.data.length,
      };
    },
    enabled: !!token && !!path && canRead && !['shortcut', 'announcement'].includes(widget.type),
  });
  if (!module) return null;
  const statusCounts = new Map<string, number>();
  if (canRead)
    for (const row of data.data?.rows ?? []) {
      const state = row.status ?? row.health_status ?? 'unknown';
      statusCounts.set(state, (statusCounts.get(state) ?? 0) + 1);
    }
  const chartData = [...statusCounts].map(([state, count]) => ({
    status: statusText(state, i18n.language),
    count,
  }));
  return (
    <section
      className="min-w-0 rounded-lg border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-4"
      style={{ minHeight: widget.height * 72 }}
    >
      <h3 className="mb-3 text-sm font-semibold">{widget.title}</h3>
      {widget.type === 'announcement' ? (
        <AnnouncementWidget />
      ) : !canRead ? (
        <p className="text-xs text-[var(--color-text-tertiary)]">
          {text('当前账号无权读取关联模块数据', 'Your account cannot read this module')}
        </p>
      ) : widget.type === 'shortcut' ? (
        <Link to={module.path} className="text-sm text-[var(--color-accent)] hover:underline">
          {localized(module.title, i18n.language)} →
        </Link>
      ) : !path ? (
        <Link to={module.path} className="text-sm text-[var(--color-accent)]">
          {text('打开关联页面', 'Open the linked page')}
        </Link>
      ) : data.error ? (
        <ErrorBanner
          message={apiErrorMessage(
            data.error,
            text('组件数据读取失败', 'Could not load widget data'),
          )}
        />
      ) : data.isPending ? (
        <p className="text-xs">{text('正在加载', 'Loading')}</p>
      ) : widget.type === 'chart' && chartData.length ? (
        <>
          <p className="mb-2 text-xs text-[var(--color-text-tertiary)]">
            {text('当前最多5条记录的状态分布', 'Status distribution of up to 5 displayed records')}
          </p>
          <div className="h-40 w-full">
            <ResponsiveContainer width="100%" height="100%">
              <BarChart data={chartData}>
                <XAxis dataKey="status" tick={{ fontSize: 11 }} />
                <YAxis allowDecimals={false} tick={{ fontSize: 11 }} />
                <Tooltip />
                <Bar dataKey="count" name={text('记录数', 'Records')} fill="var(--color-accent)" />
              </BarChart>
            </ResponsiveContainer>
          </div>
        </>
      ) : widget.type === 'metric' ? (
        <>
          <p className="text-3xl font-semibold">{data.data?.total ?? '—'}</p>
          <p className="mt-2 text-xs text-[var(--color-text-tertiary)]">
            {localized(module.title, i18n.language)} · {text('资源总数', 'Total resources')}
          </p>
        </>
      ) : (
        <>
          <ul className="space-y-2">
            {data.data?.rows.map((row) => (
              <li key={row.id} className="flex justify-between gap-2 text-xs">
                <span className="truncate">{row.name}</span>
                <span className="text-[var(--color-text-tertiary)]">
                  {statusText(row.status ?? row.health_status ?? '', i18n.language)}
                </span>
              </li>
            ))}
          </ul>
          {!data.data?.rows.length && (
            <p className="text-xs text-[var(--color-text-tertiary)]">
              {text('暂无记录', 'No records')}
            </p>
          )}
          <Link to={module.path} className="mt-3 block text-xs text-[var(--color-accent)]">
            {text('查看全部', 'View all')} →
          </Link>
        </>
      )}
    </section>
  );
}
