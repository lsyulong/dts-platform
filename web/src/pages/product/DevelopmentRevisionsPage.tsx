import { useEffect, useRef, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { ErrorBanner } from '../../components/ui';
import { Button } from '../../components/ui/button';
import ProductModulePage from '../../product/ProductModulePage';
import type { ProductModule } from '../../product/catalog-types';
import type { ResourcePage } from '../../product/resource-types';
import { useRevisionQuery, useRevisionSession } from '../../product/development-revisions/api';
import type { RevisionKind } from '../../product/development-revisions/types';
import DevelopmentRevisionPanel from './DevelopmentRevisionPanel';
export default function DevelopmentRevisionsPage({ module }: { module: ProductModule }) {
  const session = useRevisionSession();
  return <Page key={`${session.key}:${module.id}`} module={module} />;
}
function Page({ module }: { module: ProductModule }) {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const session = useRevisionSession();
  const [params, setParams] = useSearchParams();
  const id = params.get('revision_source') ?? '';
  const [offset, setOffset] = useState(0),
    [search, setSearch] = useState(''),
    [q, setQ] = useState('');
  const panel = useRef<HTMLDivElement>(null);
  const [sequence, setSequence] = useState(0);
  const [requestSave, setRequestSave] = useState<{
    sourceId: string;
    sequence: number;
    expectedVersion: number;
  } | null>(null);
  const [navigationError, setNavigationError] = useState('');
  const setId = (value: string) => {
    const next = new URLSearchParams(params);
    if (value) next.set('revision_source', value);
    else next.delete('revision_source');
    setParams(next, { replace: true });
  };
  useEffect(() => {
    if (id) panel.current?.scrollIntoView?.({ behavior: 'smooth', block: 'start' });
  }, [id, sequence]);
  const supported =
    module.resourceKind === 'sql_document' || module.resourceKind === 'development_workflow';
  const kind: RevisionKind =
    module.resourceKind === 'development_workflow' ? 'development_workflow' : 'sql_document';
  const resources = useRevisionQuery<ResourcePage>(
    `/product/resources/${kind}`,
    { q, limit: 20, offset },
    supported,
    (page, tenant) => {
      if (page.data.some((r) => r.tenant_id !== tenant || r.kind !== kind))
        throw new Error('Invalid development directory');
    },
  );
  if (!supported || !session.ready || !session.canRead)
    return (
      <ErrorBanner
        message={text(
          '需要当前有效身份和开发查看权限',
          'An active identity and development read permission are required',
        )}
      />
    );
  const page = !resources.error ? resources.data : undefined;
  return (
    <div className="space-y-6">
      <ProductModulePage
        module={module}
        onActionNavigate={(record, action) => {
          if (action !== 'save_version') return false;
          if (record.tenant_id !== session.tenantId || record.kind !== kind || !session.canWrite) {
            setNavigationError(
              text(
                '修订来源或当前权限已失效',
                'Revision source or current permission is unavailable',
              ),
            );
            return true;
          }
          setNavigationError('');
          setId(record.id);
          const nextSequence = sequence + 1;
          setSequence(nextSequence);
          setRequestSave({
            sourceId: record.id,
            sequence: nextSequence,
            expectedVersion: record.version,
          });
          return true;
        }}
      />
      {navigationError && <ErrorBanner message={navigationError} />}
      <section className="data-panel p-4 space-y-3">
        <form
          className="flex gap-2"
          onSubmit={(event) => {
            event.preventDefault();
            setQ(search.trim());
            setOffset(0);
          }}
        >
          <input
            aria-label={text('搜索修订来源', 'Search revision source')}
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            className="rounded border border-[var(--color-border-default)] p-2"
          />
          <Button type="submit" variant="outline">
            {text('搜索', 'Search')}
          </Button>
        </form>
        {resources.error ? (
          <ErrorBanner message={text('来源目录读取失败', 'Failed to load source directory')} />
        ) : (
          <>
            <label>
              {text('选择定义查看修订', 'Definition for revision history')}{' '}
              <select
                aria-label={text('选择定义查看修订', 'Definition for revision history')}
                value={id}
                onChange={(event) => setId(event.target.value)}
                className="rounded border border-[var(--color-border-default)] p-2"
              >
                <option value="">{text('请选择实际定义', 'Select an actual definition')}</option>
                {id && !page?.data.some((r) => r.id === id) && (
                  <option value={id}>
                    {text('已选定义', 'Selected definition')} · {id}
                  </option>
                )}
                {page?.data.map((r) => (
                  <option key={r.id} value={r.id}>
                    {r.code} · {r.name} · v{r.version}
                  </option>
                ))}
              </select>
            </label>
            {!page && <p>{text('正在读取来源目录…', 'Loading source directory…')}</p>}
            <div className="flex gap-2 text-sm items-center">
              <span>{page ? `${page.total}` : '—'}</span>
              <Button
                variant="outline"
                disabled={!offset || resources.isFetching}
                onClick={() => setOffset(Math.max(0, offset - 20))}
              >
                {text('上一页', 'Previous')}
              </Button>
              <Button
                variant="outline"
                disabled={!page || offset + 20 >= page.total || resources.isFetching}
                onClick={() => setOffset(offset + 20)}
              >
                {text('下一页', 'Next')}
              </Button>
            </div>
          </>
        )}
      </section>
      <div ref={panel}>
        {id && !resources.error && (
          <DevelopmentRevisionPanel
            kind={kind}
            resourceId={id}
            requestSave={requestSave?.sourceId === id ? requestSave : undefined}
            onSaveRequestHandled={() => setRequestSave(null)}
          />
        )}
      </div>
    </div>
  );
}
