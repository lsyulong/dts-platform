import { useEffect, useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { Button } from '../../components/ui/button';
import { ErrorBanner } from '../../components/ui';
import {
  useRevisionCurrent,
  useRevisionDetail,
  useRevisionHistory,
  useRevisionSession,
  revisionKey,
} from '../../product/development-revisions/api';
import { useRevisionOperations } from '../../product/development-revisions/operations';
import type { RevisionKind } from '../../product/development-revisions/types';
import RevisionComparison from '../../product/development-revisions/Comparison';
import RevisionConfirm from '../../product/development-revisions/Confirm';
import RevisionSnapshot from '../../product/development-revisions/Snapshot';

export default function DevelopmentRevisionPanel({
  kind,
  resourceId,
  requestSave,
  onSaveRequestHandled,
}: {
  kind: RevisionKind;
  resourceId: string;
  requestSave?: { sequence: number; expectedVersion: number };
  onSaveRequestHandled?: () => void;
}) {
  const session = useRevisionSession();
  return (
    <Panel
      key={`${session.key}:${kind}:${resourceId}`}
      kind={kind}
      resourceId={resourceId}
      requestSave={requestSave}
      onSaveRequestHandled={onSaveRequestHandled}
    />
  );
}
function Panel({
  kind,
  resourceId,
  requestSave,
  onSaveRequestHandled,
}: {
  kind: RevisionKind;
  resourceId: string;
  requestSave?: { sequence: number; expectedVersion: number };
  onSaveRequestHandled?: () => void;
}) {
  const cache = useQueryClient();
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const session = useRevisionSession();
  const [offset, setOffset] = useState(0),
    [selected, setSelected] = useState('');
  const [comparison, setComparison] = useState<{ from: string; to: string } | null>(null),
    [compareTo, setCompareTo] = useState('');
  const current = useRevisionCurrent(kind, resourceId),
    history = useRevisionHistory(kind, resourceId, offset);
  const detail = useRevisionDetail(kind, resourceId, selected);
  const operations = useRevisionOperations(session, kind, resourceId);
  const { begin: beginOperation, busy: operationBusy } = operations;
  const handledRequest = useRef<number | null>(null);
  useEffect(() => {
    if (
      !requestSave ||
      operationBusy ||
      handledRequest.current === requestSave.sequence ||
      !current.data ||
      current.error ||
      !session.ready ||
      !session.canRead ||
      !session.canWrite
    )
      return;
    handledRequest.current = requestSave.sequence;
    beginOperation({
      action: 'save',
      version: requestSave.expectedVersion,
      revisionId: null,
      reason: '',
    });
    onSaveRequestHandled?.();
  }, [
    requestSave,
    current.data,
    current.error,
    session.ready,
    session.canRead,
    session.canWrite,
    beginOperation,
    operationBusy,
    onSaveRequestHandled,
  ]);
  if (!session.ready || !session.canRead)
    return (
      <ErrorBanner
        message={text(
          '需要当前有效身份和开发查看权限',
          'An active identity and development read permission are required',
        )}
      />
    );
  const actual = !current.error ? current.data : undefined;
  const revisions = !history.error ? history.data : undefined;
  const revision = !detail.error ? detail.data : undefined;
  const editable = actual && ['draft', 'inactive'].includes(actual.status);
  return (
    <section className="space-y-4">
      <div className="data-panel p-4 space-y-3">
        <h2 className="text-lg font-semibold">
          {text('定义修订历史', 'Definition revision history')}
        </h2>
        <p className="text-sm">
          {text(
            '保存修订仅归档定义。恢复生成新草稿，需另行发布和执行；已提交运行使用其原有不可变快照。',
            'Saving archives a definition. Restore creates a new draft that must be published and executed separately; submitted runs retain their original immutable snapshots.',
          )}
        </p>
        {current.error ? (
          <ErrorBanner message={text('当前定义读取失败', 'Failed to load current definition')} />
        ) : actual ? (
          <p>
            {actual.name} · v{actual.version} · {actual.status}
          </p>
        ) : (
          <p>{text('正在读取当前定义…', 'Loading current definition…')}</p>
        )}
        <div className="flex gap-2">
          <Button
            disabled={!actual || !session.canWrite || operations.busy}
            onClick={() =>
              actual &&
              operations.begin({
                action: 'save',
                version: actual.version,
                reason: '',
                revisionId: null,
              })
            }
          >
            {text('保存当前修订', 'Save current revision')}
          </Button>
          <Button
            variant="outline"
            disabled={current.isFetching || history.isFetching}
            onClick={() => {
              void cache.invalidateQueries({ queryKey: revisionKey(session) });
            }}
          >
            {text('刷新修订', 'Refresh revisions')}
          </Button>
        </div>
        {operations.result && (
          <p role="status" className="text-sm break-all">
            {text('真实回执', 'Receipt')}: {operations.result.receipt_id} ·{' '}
            {operations.result.replayed
              ? text('已重放原结果', 'Original result replayed')
              : text('已保存', 'Saved')}
            {'created' in operations.result && !operations.result.created && (
              <>
                {' '}
                ·{' '}
                {text(
                  '该版本已存在修订，返回原始记录',
                  'This version is already archived; the original revision was returned',
                )}
              </>
            )}
            {'resource' in operations.result && (
              <>
                {' '}
                · {text('恢复为新草稿', 'Restored as a new draft')} v
                {operations.result.resource.version}
              </>
            )}
          </p>
        )}
      </div>
      <div className="data-panel p-4 space-y-3">
        {history.error ? (
          <ErrorBanner
            message={text('修订历史读取失败，请重试', 'Failed to load revision history')}
          />
        ) : revisions ? (
          <>
            {revisions.data.length ? (
              <div className="overflow-x-auto">
                <table className="w-full text-sm">
                  <thead>
                    <tr>
                      <th>{text('来源版本', 'Source version')}</th>
                      <th>{text('保存者与时间', 'Actor and time')}</th>
                      <th>{text('原因', 'Reason')}</th>
                      <th>{text('操作', 'Actions')}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {revisions.data.map((r) => (
                      <tr key={r.id}>
                        <td className="p-2">v{r.source_version}</td>
                        <td className="p-2">
                          {r.saved_by}
                          <br />
                          {r.saved_at}
                        </td>
                        <td className="p-2">{r.reason}</td>
                        <td className="p-2">
                          <Button
                            variant="outline"
                            onClick={() => {
                              setSelected(r.id);
                              setComparison(null);
                              setCompareTo('');
                            }}
                          >
                            {text('查看修订', 'View revision')} v{r.source_version}
                          </Button>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            ) : (
              <p>{text('该定义尚未保存修订', 'This definition has no saved revisions')}</p>
            )}
            <div className="flex justify-between items-center text-sm">
              <span>
                {revisions.total} {text('条真实修订', 'actual revisions')}
              </span>
              <div className="flex gap-2">
                <Button
                  variant="outline"
                  disabled={!offset || history.isFetching}
                  onClick={() => setOffset(Math.max(0, offset - 20))}
                >
                  {text('上一页', 'Previous')}
                </Button>
                <Button
                  variant="outline"
                  disabled={offset + 20 >= revisions.total || history.isFetching}
                  onClick={() => setOffset(offset + 20)}
                >
                  {text('下一页', 'Next')}
                </Button>
              </div>
            </div>
          </>
        ) : (
          <p>{text('正在读取修订历史…', 'Loading revision history…')}</p>
        )}
      </div>
      {selected && (
        <>
          {detail.error ? (
            <ErrorBanner message={text('修订详情读取失败', 'Failed to load revision detail')} />
          ) : revision ? (
            <RevisionSnapshot revision={revision} />
          ) : (
            <p>{text('正在读取不可变快照…', 'Loading immutable snapshot…')}</p>
          )}
          {revision && (
            <div className="data-panel p-4 space-y-3">
              <div className="flex flex-wrap gap-3 items-center">
                <label>
                  {text('对比目标', 'Compare with')}{' '}
                  <select
                    aria-label={text('对比目标', 'Compare with')}
                    value={compareTo}
                    onChange={(event) => setCompareTo(event.target.value)}
                    className="rounded border border-[var(--color-border-default)] p-2"
                  >
                    <option value="">{text('当前定义', 'Current definition')}</option>
                    {compareTo &&
                      !revisions?.data.some((r) => r.id === compareTo && r.id !== selected) && (
                        <option value={compareTo}>
                          {text('已选历史修订', 'Selected revision')} · {compareTo}
                        </option>
                      )}
                    {revisions?.data
                      .filter((r) => r.id !== selected)
                      .map((r) => (
                        <option key={r.id} value={r.id}>
                          v{r.source_version} · {r.saved_at}
                        </option>
                      ))}
                  </select>
                </label>
                <Button
                  variant="outline"
                  onClick={() => setComparison({ from: selected, to: compareTo })}
                >
                  {text('查看差异', 'Compare definitions')}
                </Button>
                <Button
                  disabled={!session.canWrite || !editable || operations.busy}
                  onClick={() =>
                    actual &&
                    operations.begin({
                      action: 'restore',
                      version: actual.version,
                      revisionId: selected,
                      reason: '',
                    })
                  }
                >
                  {text('恢复此修订', 'Restore this revision')}
                </Button>
              </div>
              {actual && !editable && (
                <p className="text-sm">
                  {text(
                    '当前定义已发布或正在发布，须先完成撤回再恢复。',
                    'Withdraw the published definition before restoring.',
                  )}
                </p>
              )}
              <p className="text-xs">
                {text(
                  '对比目标修订选项为当前历史页；切页可继续查看其他真实修订。',
                  'Revision choices come from the current history page; paginate to view other actual revisions.',
                )}
              </p>
            </div>
          )}
        </>
      )}
      {comparison && (
        <RevisionComparison
          key={`${comparison.from}:${comparison.to}`}
          kind={kind}
          resourceId={resourceId}
          from={comparison.from}
          to={comparison.to}
        />
      )}
      <RevisionConfirm
        pending={session.canWrite ? operations.pending : null}
        busy={operations.busy}
        error={operations.error}
        onChange={operations.setPending}
        onClose={operations.close}
        onCommit={() => void operations.commit()}
      />
    </section>
  );
}
