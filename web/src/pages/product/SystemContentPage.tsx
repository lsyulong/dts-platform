import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '../../components/ui/button';
import { ErrorBanner } from '../../components/ui';
import { apiErrorMessage } from '../../lib/apiError';
import { useAuthStore } from '../../stores/auth';
import type { ProductModule } from '../../product/catalog-types';
import ProductModulePage from '../../product/ProductModulePage';
import { portalLayout, portalTheme, safeHex } from '../../product/design-editors/design-models';
import { useVisibleContent } from './system-content-api';
import PortalWidgetView from './PortalWidgetView';

export default function SystemContentPage({ module }: { module: ProductModule }) {
  const identity = useAuthStore((state) =>
    JSON.stringify([state.user?.tenant_id, state.user?.id, state.token]),
  );
  return <SystemContent key={`${identity}:${module.id}`} module={module} />;
}

function SystemContent({ module }: { module: ProductModule }) {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const canManage = useAuthStore((state) => state.user?.permissions.includes('system:read'));
  const [tab, setTab] = useState<'published' | 'management'>('published');
  const kind = module.resourceKind === 'portal' ? 'portal' : 'announcement';
  const content = useVisibleContent(kind);
  return (
    <div className="space-y-5">
      <h1 className="text-xl font-semibold">
        {kind === 'portal' ? text('门户', 'Portals') : text('公告', 'Announcements')}
      </h1>
      <div className="flex gap-2">
        <Button
          variant={tab === 'published' ? 'default' : 'outline'}
          onClick={() => setTab('published')}
        >
          {text('我可查看的内容', 'Available to me')}
        </Button>
        {canManage && (
          <Button
            variant={tab === 'management' ? 'default' : 'outline'}
            onClick={() => setTab('management')}
          >
            {text('配置与发布', 'Configure and publish')}
          </Button>
        )}
      </div>
      {tab === 'management' && canManage ? (
        <ProductModulePage module={module} />
      ) : (
        <>
          {content.error && (
            <ErrorBanner
              message={apiErrorMessage(
                content.error,
                text('内容加载失败', 'Could not load content'),
              )}
            />
          )}{' '}
          {!content.data?.length && (
            <p className="data-panel py-14 text-center text-sm text-[var(--color-text-tertiary)]">
              {content.isPending
                ? text('正在加载', 'Loading')
                : text(
                    '当前没有在有效期内且对您授权发布的内容',
                    'No currently published content is available to your account',
                  )}
            </p>
          )}
          {content.data?.map((item) => {
            if (kind === 'announcement')
              return (
                <article key={item.id} className="data-panel p-5">
                  <h2 className="font-semibold">{item.name}</h2>
                  <time
                    className="mt-2 block text-xs text-[var(--color-text-tertiary)]"
                    dateTime={item.updated_at}
                  >
                    {new Date(item.updated_at).toLocaleString(i18n.language)}
                  </time>
                  <p className="mt-4 whitespace-pre-wrap break-words text-sm">
                    {String(item.properties.content ?? '')}
                  </p>
                </article>
              );
            const layout = portalLayout(item.properties.layout);
            const theme = portalTheme(item.properties.theme);
            return (
              <article
                key={item.id}
                className="data-panel p-5"
                style={
                  safeHex(theme.background_color)
                    ? { backgroundColor: theme.background_color }
                    : undefined
                }
              >
                <h2
                  className="mb-4 font-semibold"
                  style={safeHex(theme.primary_color) ? { color: theme.primary_color } : undefined}
                >
                  {theme.title || item.name}
                </h2>
                <div className="grid grid-cols-12 gap-4">
                  {layout.widgets.map((widget) => (
                    <div
                      key={widget.id}
                      className="col-span-12 md:[grid-column:span_var(--widget-width)]"
                      style={{ '--widget-width': widget.width } as React.CSSProperties}
                    >
                      <PortalWidgetView widget={widget} />
                    </div>
                  ))}
                </div>
              </article>
            );
          })}
        </>
      )}
    </div>
  );
}
