import { useRef, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { RefreshCw, Play, CircleStop, ArrowRight } from 'lucide-react';
import { Button } from '../../components/ui/button';
import { ErrorBanner } from '../../components/ui';
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '../../components/ui/dialog';
import { useAuthStore } from '../../stores/auth';
import { apiErrorMessage } from '../../lib/apiError';
import { captureSession, isCurrentSession } from '../../lib/session';
import { statusText } from '../../product/ui-text';
import { useProductJob, useProductJobAction, useProductJobs, type ProductJob } from './jobs-api';
import JobNodeResult from './JobNodeResult';
import JobResultExports from './JobResultExports';
import BiShareCreate from './BiShareCreate';

function NodeResults({ job }: { job: ProductJob }) {
  const { i18n } = useTranslation();
  return (
    <div className="mt-4 space-y-3">
      {job.nodes.map((node) => (
        <section
          key={node.id}
          className="rounded-md border border-[var(--color-border-default)] p-4"
        >
          <div className="flex justify-between gap-2 text-sm">
            <strong>
              {node.name} · {node.id}
            </strong>
            <span>{statusText(node.status, i18n.language)}</span>
          </div>
          {!!node.depends_on.length && (
            <p className="mt-2 flex items-center gap-2 text-xs text-[var(--color-text-tertiary)]">
              {node.depends_on.join(', ')}
              <ArrowRight size={12} />
              {node.id}
            </p>
          )}
          {node.error && (
            <div className="mt-2">
              <ErrorBanner message={node.error} />
            </div>
          )}
          {node.result_access === 'restricted' && (
            <p className="mt-3 rounded-md bg-[var(--color-bg-secondary)] p-3 text-sm text-[var(--color-text-secondary)]">
              {i18n.language.startsWith('zh')
                ? node.result_access_reason === 'runtime_configuration_invalidated'
                  ? '运行配置已撤回或固定版本失效。请检查配置并重新发布开发定义；历史运行记录保留。'
                  : node.result_access_reason === 'publication_withdrawn'
                    ? 'API 已撤回或发布版本变化，当前结果不可查看。'
                    : '当前账号无权查看该节点的执行数据，请检查有效授权与元数据版本。'
                : node.result_access_reason === 'runtime_configuration_invalidated'
                  ? 'The frozen runtime configuration is invalid. Check the configuration and republish the definition. Run history is retained.'
                  : node.result_access_reason === 'publication_withdrawn'
                    ? 'The API was withdrawn or its publication changed. These results are unavailable.'
                    : 'Your account cannot view this node’s data. Check active grants and the metadata version.'}
            </p>
          )}
          {node.result !== null && <JobNodeResult result={node.result} />}
          {job.source_kind === 'bi_report' &&
            job.status === 'succeeded' &&
            node.status === 'succeeded' &&
            node.result_access === 'authorized' &&
            node.result !== null && (
              <BiShareCreate
                reportId={job.source_id}
                version={job.source_version}
                jobId={job.id}
                nodeId={node.id}
              />
            )}
          {job.status === 'succeeded' &&
            node.status === 'succeeded' &&
            node.result_access === 'authorized' &&
            node.result !== null &&
            ['bi_report', 'data_api', 'sql_document', 'development_workflow'].includes(
              job.source_kind,
            ) && (
              <JobResultExports
                jobId={job.id}
                nodeId={node.id}
                sourceVersion={job.source_version}
              />
            )}
        </section>
      ))}
    </div>
  );
}

export type ProductJobScope =
  'development' | 'development_tasks' | 'quality' | 'metadata' | 'bi' | 'services';
const runSources = {
  development: { kind: 'development_workflow', title: ['工作流运维', 'Workflow operations'] },
  development_tasks: { kind: 'sql_document', title: ['任务运维', 'Development task operations'] },
  quality: { kind: 'quality_task', title: ['质量校验结果', 'Quality check results'] },
  metadata: { kind: 'metadata_collection', title: ['元数据采集运行', 'Metadata collection runs'] },
  bi: { kind: 'bi_report', title: ['BI 分析运行', 'BI analysis runs'] },
  services: { kind: 'data_api', title: ['数据 API 测试调用', 'Data API test invocations'] },
};

export default function JobRunsPage({ scope = 'development' }: { scope?: ProductJobScope }) {
  const identity = useAuthStore((state) =>
    JSON.stringify([state.user?.tenant_id, state.user?.id, state.token, state.user?.permissions]),
  );
  return <JobRuns key={`${identity}:${scope}`} scope={scope} />;
}

function JobRuns({ scope }: { scope: ProductJobScope }) {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const permissions = useAuthStore((state) => state.user?.permissions ?? []);
  const [kind, setKind] = useState(runSources[scope].kind);
  const [status, setStatus] = useState('');
  const [offset, setOffset] = useState(0);
  const [searchParams, setSearchParams] = useSearchParams();
  const selectedId = searchParams.get('job') ?? undefined;
  const selectJob = (id?: string) => {
    setSearchParams((current) => {
      const next = new URLSearchParams(current);
      if (id) next.set('job', id);
      else next.delete('job');
      return next;
    });
  };
  const [pending, setPending] = useState<{
    job: ProductJob;
    action: 'cancel' | 'retry';
    key: string;
  } | null>(null);
  const [error, setError] = useState('');
  const retryKeys = useRef(new Map<string, string>());
  const permissionScope = scope === 'development_tasks' ? 'development' : scope;
  const canRead = permissions.includes(`${permissionScope}:read`);
  const canExecute = permissions.includes(`${permissionScope}:execute`);
  const runs = useProductJobs(kind, status, offset, canRead);
  const selectedRun = useProductJob(selectedId, canRead);
  const mutation = useProductJobAction();
  const detail =
    canRead &&
    !selectedRun.error &&
    selectedRun.isFetchedAfterMount &&
    selectedRun.data?.source_kind === kind &&
    selectedRun.data?.id === selectedId
      ? selectedRun.data
      : null;
  const commit = async () => {
    if (!pending || mutation.isPending || !canExecute) return;
    const session = captureSession();
    setError('');
    try {
      await mutation.mutateAsync({ id: pending.job.id, action: pending.action, key: pending.key });
      if (isCurrentSession(session)) {
        if (pending.action === 'retry') retryKeys.current.delete(pending.job.id);
        setPending(null);
      }
    } catch (caught) {
      if (isCurrentSession(session))
        setError(apiErrorMessage(caught, text('运行操作失败', 'Run operation failed')));
    }
  };
  return (
    <div className="space-y-5">
      <div>
        <h1 className="text-xl font-semibold">
          {text(runSources[scope].title[0], runSources[scope].title[1])}
        </h1>
        <p className="mt-1 text-sm text-[var(--color-text-tertiary)]">
          {text(
            '查看实际运行、节点依赖、执行结果及取消状态',
            'Inspect persisted runs, node dependencies, results and cancellation state',
          )}
        </p>
      </div>
      {!canRead && (
        <ErrorBanner
          message={text('当前账号没有查看权限', 'Your account cannot read these runs')}
        />
      )}
      <section className="data-panel">
        <div className="data-toolbar">
          <div className="flex gap-2">
            {scope === 'development' && (
              <select
                aria-label={text('运行来源', 'Run source')}
                value={kind}
                onChange={(event) => {
                  setKind(event.target.value);
                  setOffset(0);
                  selectJob();
                }}
                className="rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-2 text-sm"
              >
                <option value="development_workflow">{text('工作流', 'Workflow')}</option>
                <option value="sql_document">{text('SQL 文档', 'SQL document')}</option>
              </select>
            )}
            <select
              aria-label={text('运行状态', 'Run status')}
              value={status}
              onChange={(event) => {
                setStatus(event.target.value);
                setOffset(0);
                selectJob();
              }}
              className="rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-2 text-sm"
            >
              <option value="">{text('全部状态', 'All statuses')}</option>
              {[
                'queued',
                'running',
                'succeeded',
                'failed',
                'cancelled',
                'reconciliation_required',
              ].map((value) => (
                <option key={value} value={value}>
                  {statusText(value, i18n.language)}
                </option>
              ))}
            </select>
          </div>
          <Button
            variant="ghost"
            aria-label={text('刷新运行记录', 'Refresh runs')}
            disabled={!canRead || runs.isFetching}
            onClick={() => void runs.refetch()}
          >
            <RefreshCw size={16} />
          </Button>
        </div>
        {runs.error && (
          <div className="p-4">
            <ErrorBanner
              message={apiErrorMessage(runs.error, text('运行记录加载失败', 'Could not load runs'))}
            />
          </div>
        )}
        <div className="overflow-auto">
          <table className="w-full text-left text-sm">
            <thead className="bg-[var(--color-bg-secondary)] text-[var(--color-text-tertiary)]">
              <tr>
                {[
                  text('运行 ID / 来源', 'Run / source'),
                  text('状态', 'Status'),
                  text('节点完成情况', 'Completed nodes'),
                  text('创建时间', 'Created'),
                  text('操作', 'Actions'),
                ].map((label) => (
                  <th key={label} className="px-4 py-3 font-medium">
                    {label}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {canRead &&
                !runs.error &&
                runs.data?.data.map((job) => (
                  <tr key={job.id} className="border-t border-[var(--color-border-default)]">
                    <td className="px-4 py-3">
                      <button
                        className="text-[var(--color-accent)] hover:underline"
                        onClick={() => selectJob(job.id)}
                      >
                        {job.id}
                      </button>
                      <p className="mt-1 text-xs text-[var(--color-text-tertiary)]">
                        {job.source_id} · v{job.source_version}
                      </p>
                    </td>
                    <td className="px-4 py-3">
                      {statusText(job.status, i18n.language)}
                      {job.cancel_requested && (
                        <p className="text-xs">{text('已请求取消', 'Cancellation requested')}</p>
                      )}
                    </td>
                    <td className="px-4 py-3">
                      {job.nodes.filter((node) => node.status === 'succeeded').length} /{' '}
                      {job.nodes.length}
                    </td>
                    <td className="px-4 py-3">
                      {new Date(job.created_at).toLocaleString(i18n.language)}
                    </td>
                    <td className="px-4 py-3">
                      {canExecute && ['queued', 'running'].includes(job.status) && (
                        <Button
                          variant="ghost"
                          disabled={job.cancel_requested}
                          onClick={() => {
                            setPending({ job, action: 'cancel', key: crypto.randomUUID() });
                            setError('');
                          }}
                        >
                          <CircleStop size={14} />
                          {text('取消', 'Cancel')}
                        </Button>
                      )}
                      {canExecute &&
                        job.can_retry &&
                        ['failed', 'cancelled'].includes(job.status) && (
                          <Button
                            variant="ghost"
                            onClick={() => {
                              const key = retryKeys.current.get(job.id) ?? crypto.randomUUID();
                              retryKeys.current.set(job.id, key);
                              setPending({ job, action: 'retry', key });
                              setError('');
                            }}
                          >
                            <Play size={14} />
                            {text('重试', 'Retry')}
                          </Button>
                        )}
                    </td>
                  </tr>
                ))}
            </tbody>
          </table>
        </div>
        {canRead && !runs.error && !runs.data?.data.length && (
          <p className="py-14 text-center text-sm text-[var(--color-text-tertiary)]">
            {runs.isPending && canRead
              ? text('正在加载', 'Loading')
              : text('没有符合条件的实际运行记录', 'No persisted runs match the filter')}
          </p>
        )}
        <div className="flex justify-between border-t border-[var(--color-border-default)] p-4 text-sm">
          <span>
            {canRead && !runs.error && runs.data
              ? `${runs.data.total} ${text('次运行', 'runs')}`
              : '—'}
          </span>
          <div className="flex gap-2">
            <Button
              variant="outline"
              disabled={!canRead || !offset || runs.isFetching}
              onClick={() => setOffset(Math.max(0, offset - 20))}
            >
              {text('上一页', 'Previous')}
            </Button>
            <Button
              variant="outline"
              disabled={
                !canRead ||
                !!runs.error ||
                !runs.data ||
                offset + 20 >= runs.data.total ||
                runs.isFetching
              }
              onClick={() => setOffset(offset + 20)}
            >
              {text('下一页', 'Next')}
            </Button>
          </div>
        </div>
      </section>
      <Dialog
        open={canRead && !!selectedId}
        onOpenChange={(open) => {
          if (!open) selectJob();
        }}
      >
        <DialogContent className="max-w-4xl">
          <DialogTitle>{text('运行详情', 'Run details')}</DialogTitle>
          <DialogDescription>
            {detail?.id} · {statusText(detail?.status ?? '', i18n.language)}
          </DialogDescription>
          {selectedRun.error && (
            <ErrorBanner
              message={apiErrorMessage(
                selectedRun.error,
                text('运行详情加载失败', 'Could not load run details'),
              )}
            />
          )}
          {selectedRun.isPending && (
            <p role="status" className="text-sm text-[var(--color-text-tertiary)]">
              {text('正在加载运行详情', 'Loading run details')}
            </p>
          )}
          {selectedRun.data && selectedRun.data.source_kind !== kind && !selectedRun.error && (
            <ErrorBanner
              message={text(
                '该运行不属于当前选择的任务类型，请从对应模块查看。',
                'This run belongs to a different source kind. Open its corresponding workspace.',
              )}
            />
          )}
          {detail?.error && <ErrorBanner message={detail.error} />}
          {detail && (
            <div className="max-h-[65vh] overflow-auto">
              <NodeResults job={detail} />
            </div>
          )}
        </DialogContent>
      </Dialog>
      <Dialog
        open={canExecute && !!pending}
        onOpenChange={(open) => {
          if (!open && !mutation.isPending) setPending(null);
        }}
      >
        <DialogContent>
          <DialogTitle>
            {pending?.action === 'cancel'
              ? text('取消当前运行', 'Cancel this run')
              : text('创建重试运行', 'Retry this run')}
          </DialogTitle>
          <DialogDescription>
            {pending?.action === 'cancel'
              ? text(
                  '保存取消请求，执行器会在受控边界停止，确认后记录最终状态。',
                  'The executor observes the persisted cancellation request before recording its final state.',
                )
              : text(
                  '保留原版本快照并创建独立运行记录；定义变化时将拒绝重试。',
                  'Retry creates a separate run using the original snapshot and rejects changed definitions.',
                )}
          </DialogDescription>
          {error && <ErrorBanner message={error} />}
          <div className="mt-4 flex justify-end gap-2">
            <Button
              variant="outline"
              disabled={mutation.isPending}
              onClick={() => setPending(null)}
            >
              {text('返回', 'Back')}
            </Button>
            <Button disabled={mutation.isPending} onClick={() => void commit()}>
              {text('确认', 'Confirm')}
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}
