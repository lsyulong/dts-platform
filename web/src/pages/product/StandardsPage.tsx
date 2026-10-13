import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useSearchParams } from 'react-router-dom';
import type { ProductModule } from '../../product/catalog-types';
import ProductModulePage from '../../product/ProductModulePage';
import { useProductResources } from '../../product/api';
import { useAuthStore } from '../../stores/auth';
import { Button, ErrorBanner } from '../../components/ui';
import { captureSession, isCurrentSession } from '../../lib/session';
import { apiErrorMessage } from '../../lib/apiError';
import {
  standardIdentity,
  standardKinds,
  useStandardOperation,
  useStandardRevisions,
  type StandardKind,
  type StandardRevision,
} from './standards-api';

export default function StandardsPage({ module }: { module: ProductModule }) {
  const identity = useAuthStore((s) =>
    JSON.stringify([s.user?.tenant_id, s.user?.id, s.token, s.user?.permissions]),
  );
  return <Standards key={`${identity}:${module.id}`} module={module} />;
}
function Standards({ module }: { module: ProductModule }) {
  const { i18n } = useTranslation(),
    text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const [params, setParams] = useSearchParams(),
    [tab, setTab] = useState<'definitions' | 'revisions'>(
      params.has('standard') ? 'revisions' : 'definitions',
    );
  const activeTab = params.has('standard') ? 'revisions' : tab;
  if (!standardKinds.some((k) => k === module.resourceKind))
    return (
      <ErrorBanner
        message={text('此目录不是标准定义', 'This directory is not a standard definition')}
      />
    );
  const kind = module.resourceKind as StandardKind;
  const choose = (value: 'definitions' | 'revisions') => {
    setTab(value);
    if (value === 'definitions') {
      const next = new URLSearchParams(params);
      next.delete('standard');
      setParams(next);
    }
  };
  return (
    <div className="space-y-5">
      <div
        role="tablist"
        aria-label={text('标准治理', 'Standard governance')}
        className="flex gap-3 border-b border-[var(--color-border-default)]"
      >
        {(['definitions', 'revisions'] as const).map((v) => (
          <button
            key={v}
            role="tab"
            aria-selected={activeTab === v}
            onClick={() => choose(v)}
            className={`border-b-2 px-4 py-3 ${activeTab === v ? 'border-[var(--color-accent)] text-[var(--color-accent)]' : 'border-transparent'}`}
          >
            {v === 'definitions'
              ? text('定义与发布', 'Definition and publication')
              : text('已发布版本与撤回', 'Published revisions and withdrawal')}
          </button>
        ))}
      </div>
      {activeTab === 'definitions' ? (
        <ProductModulePage
          module={module}
          onActionNavigate={(record, action) => {
            if (action !== 'withdraw') return false;
            const next = new URLSearchParams(params);
            next.set('standard', record.id);
            setParams(next);
            setTab('revisions');
            return true;
          }}
        />
      ) : (
        <RevisionHistory kind={kind} />
      )}
    </div>
  );
}
function RevisionHistory({ kind }: { kind: StandardKind }) {
  const { i18n } = useTranslation(),
    text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const user = useAuthStore((s) => s.user),
    canRead = !!user?.permissions.includes('standards:read'),
    canWrite =
      !!user?.permissions.includes('standards:write') &&
      !!user.permissions.includes('standards:execute');
  const [params, setParams] = useSearchParams(),
    chosen = params.get('standard') ?? '',
    [offset, setOffset] = useState(0),
    [resourceOffset, setResourceOffset] = useState(0),
    [error, setError] = useState(''),
    [pending, setPending] = useState<StandardRevision | null>(null),
    [reason, setReason] = useState('');
  const resources = useProductResources(kind, { limit: 20, offset: resourceOffset }, canRead),
    revisions = useStandardRevisions(kind, chosen || undefined, offset),
    operation = useStandardOperation();
  const actual = !resources.error
      ? (resources.data?.data ?? []).filter(
          (r) => r.tenant_id === user?.tenant_id && r.kind === kind,
        )
      : [],
    source = actual.find((r) => r.id === chosen);
  const rows = canRead && !resources.error && !revisions.error ? (revisions.data?.data ?? []) : [];
  const withdrawn = async () => {
    if (!pending || !source || !canWrite || operation.isPending || !reason.trim()) return;
    const session = captureSession(),
      identity = standardIdentity();
    setError('');
    try {
      await operation.mutateAsync({
        type: 'standard',
        kind,
        id: source.id,
        version: source.version,
        action: 'withdraw',
        payload: {
          revision_version: pending.revision_version,
          expected_state_version: pending.state_version,
          reason: reason.trim(),
        },
      });
      if (isCurrentSession(session) && identity === standardIdentity()) {
        setPending(null);
        setReason('');
      }
    } catch (e) {
      if (isCurrentSession(session) && identity === standardIdentity())
        setError(
          apiErrorMessage(
            e,
            text(
              '撤回失败；实际版本状态未确认',
              'Withdrawal failed; the actual revision state is unconfirmed',
            ),
          ),
        );
    }
  };
  if (!canRead)
    return (
      <ErrorBanner
        message={text('当前账号没有标准查看权限', 'Your account cannot read standards')}
      />
    );
  return (
    <section className="space-y-4">
      <h1 className="text-xl font-semibold">
        {text('不可变发布版本', 'Immutable published revisions')}
      </h1>
      <p className="text-sm text-[var(--color-text-secondary)]">
        {text(
          '修订保留旧版本引用；撤回会阻止该版本及子标准的后续发布和执行，不会修改已有物理数据。',
          'Revision keeps older pinned releases available. Withdrawal prevents future publication and execution using that release or its children, and does not modify existing physical data.',
        )}
      </p>
      <label className="block text-sm">
        {text('标准定义', 'Standard definition')}
        <select
          aria-label={text('标准定义', 'Standard definition')}
          className="mt-2 w-full rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] px-3 py-2"
          value={chosen}
          onChange={(e) => {
            const next = new URLSearchParams(params);
            if (e.target.value) next.set('standard', e.target.value);
            else next.delete('standard');
            setParams(next);
            setOffset(0);
            setPending(null);
          }}
        >
          <option value="">
            {text('选择实际标准定义', 'Select an actual standard definition')}
          </option>
          {chosen && !actual.some((r) => r.id === chosen) && (
            <option value={chosen}>{chosen}</option>
          )}
          {actual.map((r) => (
            <option key={r.id} value={r.id}>
              {r.name} · {r.code} · v{r.version}
            </option>
          ))}
        </select>
      </label>
      <div className="flex gap-2">
        <Button
          variant="outline"
          disabled={!resourceOffset || resources.isFetching}
          onClick={() => setResourceOffset(Math.max(0, resourceOffset - 20))}
        >
          {text('上一页定义', 'Previous definitions')}
        </Button>
        <Button
          variant="outline"
          disabled={
            !!resources.error ||
            resourceOffset + 20 >= (resources.data?.total ?? 0) ||
            resources.isFetching
          }
          onClick={() => setResourceOffset(resourceOffset + 20)}
        >
          {text('下一页定义', 'Next definitions')}
        </Button>
        <Button
          variant="outline"
          disabled={resources.isFetching || revisions.isFetching}
          onClick={() => {
            void resources.refetch();
            if (chosen) void revisions.refetch();
          }}
        >
          {text('刷新版本', 'Refresh revisions')}
        </Button>
      </div>
      {(error || resources.error || revisions.error) && (
        <div role="alert">
          <ErrorBanner
            message={
              error ||
              apiErrorMessage(
                resources.error || revisions.error,
                text('真实版本读取失败', 'Could not read actual revisions'),
              )
            }
          />
        </div>
      )}
      {!chosen && (
        <p>
          {text(
            '请选择标准后读取其实际发布历史。',
            'Select a standard to read its actual publication history.',
          )}
        </p>
      )}
      {chosen && revisions.isPending && (
        <p role="status">{text('正在读取发布版本…', 'Reading published revisions…')}</p>
      )}
      {chosen && !revisions.isPending && !revisions.error && rows.length === 0 && (
        <p>
          {text(
            '没有已发布版本；草稿不能作为引用版本。',
            'There are no published revisions. A draft cannot be used as a pinned revision.',
          )}
        </p>
      )}
      <div className="grid gap-4 lg:grid-cols-2">
        {rows.map((r) => (
          <article
            key={r.revision_version}
            className="rounded-lg border border-[var(--color-border-default)] bg-[var(--color-bg-primary)] p-4"
          >
            <div className="flex justify-between">
              <h2 className="font-semibold">
                {r.name} · v{r.revision_version}
              </h2>
              <span>
                {r.state === 'active'
                  ? text('有效发布', 'Active release')
                  : text('已撤回', 'Withdrawn')}
              </span>
            </div>
            <p className="mt-2 text-xs break-all">SHA-256: {r.definition_hash}</p>
            <p className="mt-2 text-xs">
              {text('发布时间', 'Published')}:{' '}
              {new Date(r.published_at).toLocaleString(i18n.language)}
            </p>
            {r.children.map((c) => (
              <p key={`${c.kind}:${c.id}`} className="mt-2 text-xs">
                {c.kind} / {c.id} · v{c.revision_version}
              </p>
            ))}
            <details className="my-3">
              <summary className="text-sm">{text('冻结定义', 'Frozen definition')}</summary>
              <pre className="mt-2 overflow-auto whitespace-pre-wrap text-xs">
                {JSON.stringify(r.properties, null, 2)}
              </pre>
            </details>
            {r.withdrawal_reason && (
              <p className="text-sm">
                {text('撤回原因', 'Withdrawal reason')}: {r.withdrawal_reason}
              </p>
            )}
            {r.state === 'active' && canWrite && (
              <Button
                variant="outline"
                disabled={!source || operation.isPending || !!resources.error || !!revisions.error}
                onClick={() => {
                  setPending(r);
                  setReason('');
                  setError('');
                }}
              >
                {text('撤回此版本', 'Withdraw this revision')}
              </Button>
            )}
          </article>
        ))}
      </div>
      {chosen && (
        <div className="flex gap-2">
          <Button
            variant="outline"
            disabled={!offset || revisions.isFetching}
            onClick={() => setOffset(Math.max(0, offset - 20))}
          >
            {text('上一页版本', 'Previous revisions')}
          </Button>
          <Button
            variant="outline"
            disabled={
              !!revisions.error ||
              offset + 20 >= (revisions.data?.total ?? 0) ||
              revisions.isFetching
            }
            onClick={() => setOffset(offset + 20)}
          >
            {text('下一页版本', 'Next revisions')}
          </Button>
        </div>
      )}
      {pending && (
        <form
          className="space-y-3 rounded-lg border border-[var(--color-border-default)] p-4"
          onSubmit={(e) => {
            e.preventDefault();
            void withdrawn();
          }}
        >
          <h2 className="font-semibold">
            {text('撤回发布版本', 'Withdraw published revision')} v{pending.revision_version}
          </h2>
          <label className="block text-sm">
            {text('撤回原因', 'Withdrawal reason')}
            <textarea
              aria-label={text('撤回原因', 'Withdrawal reason')}
              required
              maxLength={2000}
              value={reason}
              onChange={(e) => setReason(e.target.value)}
              className="mt-2 w-full rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-3"
            />
          </label>
          <div className="flex gap-2">
            <Button
              type="submit"
              disabled={
                operation.isPending ||
                !source ||
                !reason.trim() ||
                !!resources.error ||
                !!revisions.error
              }
            >
              {text('确认撤回', 'Confirm withdrawal')}
            </Button>
            <Button type="button" variant="outline" onClick={() => setPending(null)}>
              {text('取消', 'Cancel')}
            </Button>
          </div>
        </form>
      )}
    </section>
  );
}
