import { useRef, useState } from 'react';
import { Link, useSearchParams } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { useAuthStore } from '../../stores/auth';
import { captureSession, isCurrentSession } from '../../lib/session';
import { apiErrorMessage } from '../../lib/apiError';
import { Button } from '../../components/ui/button';
import { ErrorBanner } from '../../components/ui';
import type { ProductModule } from '../../product/catalog-types';
import { statusText } from '../../product/ui-text';
import ProductModulePage from '../../product/ProductModulePage';
import SensitiveRunResult from './SensitiveRunResult';
import SensitiveFindingsPage from './SensitiveFindingsPage';
import { committedSensitiveResult } from './sensitive-result';
import {
  useSensitiveAction,
  useSensitiveJob,
  useSensitiveJobs,
  useSensitiveTasks,
} from './sensitive-api';

export default function SensitiveRecognitionPage({ module }: { module: ProductModule }) {
  const identity = useAuthStore((state) =>
    JSON.stringify([state.user?.tenant_id, state.user?.id, state.token, state.user?.permissions]),
  );
  if (module.resourceKind !== 'recognition_task') return <ProductModulePage module={module} />;
  return <Recognition key={`${identity}:${module.id}`} module={module} />;
}
function Recognition({ module }: { module: ProductModule }) {
  const { i18n } = useTranslation(),
    text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const [params, setParams] = useSearchParams(),
    [tab, setTab] = useState<'definitions' | 'runs' | 'findings'>('definitions');
  const active = params.get('job') ? 'runs' : tab;
  const change = (value: typeof tab) => {
    setTab(value);
    if (params.has('job')) {
      const next = new URLSearchParams(params);
      next.delete('job');
      setParams(next);
    }
  };
  return (
    <div className="space-y-5">
      <div
        role="tablist"
        aria-label={text('敏感识别', 'Sensitive recognition')}
        className="flex gap-4 border-b border-[var(--color-border-default)]"
      >
        {(['definitions', 'runs', 'findings'] as const).map((value) => (
          <button
            key={value}
            role="tab"
            aria-selected={active === value}
            onClick={() => change(value)}
            className={`border-b-2 px-3 py-3 text-sm ${active === value ? 'border-[var(--color-accent)] text-[var(--color-accent)]' : 'border-transparent text-[var(--color-text-secondary)]'}`}
          >
            {value === 'definitions'
              ? text('任务配置', 'Task definitions')
              : value === 'runs'
                ? text('真实运行', 'Actual runs')
                : text('发现复核', 'Finding review')}
          </button>
        ))}
      </div>
      {active === 'definitions' ? (
        <ProductModulePage module={module} />
      ) : active === 'runs' ? (
        <Runs />
      ) : (
        <SensitiveFindingsPage />
      )}
    </div>
  );
}
function Runs() {
  const { i18n } = useTranslation(),
    text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const permissions = useAuthStore((state) => state.user?.permissions ?? []),
    canRead = permissions.includes('sensitive_data:read'),
    canWrite = permissions.includes('sensitive_data:write'),
    canExecute = permissions.includes('sensitive_data:execute');
  const [params, setParams] = useSearchParams(),
    [taskId, setTaskId] = useState(''),
    [status, setStatus] = useState(''),
    [offset, setOffset] = useState(0),
    [error, setError] = useState(''),
    [notice, setNotice] = useState('');
  const selected = params.get('job') ?? undefined,
    keys = useRef(new Map<string, string>()),
    tasks = useSensitiveTasks(),
    runs = useSensitiveJobs(taskId, status, offset),
    detail = useSensitiveJob(selected),
    mutation = useSensitiveAction();
  const task = !tasks.error ? tasks.data?.data.find((item) => item.id === taskId) : undefined,
    job =
      !!selected &&
      !!detail.data &&
      !detail.error &&
      !runs.error &&
      detail.data?.id === selected &&
      detail.data.source_kind === 'recognition_task'
        ? detail.data
        : null;
  const node = job?.nodes.find((node) => node.id === 'recognition'),
    result =
      job?.status === 'succeeded' &&
      job.result_access === 'authorized' &&
      node?.status === 'succeeded' &&
      node.result_access === 'authorized'
        ? committedSensitiveResult(node.result)
        : null;
  const choose = (id: string) => {
    const next = new URLSearchParams(params);
    next.set('job', id);
    setParams(next);
  };
  const keyFor = (identity: string) => {
    const known = keys.current.get(identity);
    if (known) return known;
    const key = crypto.randomUUID();
    keys.current.set(identity, key);
    return key;
  };
  const act = async (
    action: 'validate' | 'publish' | 'withdraw' | 'execute' | 'cancel' | 'retry',
  ) => {
    if (mutation.isPending || !canRead) return;
    if (['validate', 'publish', 'withdraw', 'execute'].includes(action) && !task) return;
    if (['cancel', 'retry'].includes(action) && !job) return;
    if (['publish', 'withdraw'].includes(action) && !canWrite) return;
    if (['execute', 'cancel', 'retry'].includes(action) && !canExecute) return;
    const session = captureSession();
    setError('');
    setNotice('');
    try {
      let response;
      if (action === 'cancel' || action === 'retry') {
        if (!job) return;
        response = await mutation.mutateAsync({
          job,
          action,
          key: action === 'retry' ? keyFor(`retry:${job.id}`) : undefined,
        });
      } else {
        if (!task) return;
        response = await mutation.mutateAsync({
          task,
          action,
          key: action === 'execute' ? keyFor(`execute:${task.id}:${task.version}`) : undefined,
        });
      }
      if (isCurrentSession(session)) {
        const value = response.value;
        if ('run_id' in value && typeof value.run_id === 'string') choose(value.run_id);
        else if ('job' in value) choose(value.job.id);
        if (action === 'execute' && task) keys.current.delete(`execute:${task.id}:${task.version}`);
        if (action === 'retry' && job) keys.current.delete(`retry:${job.id}`);
        setNotice(
          action === 'execute'
            ? text(
                '实际运行已提交，等待领取与执行。',
                'Actual run submitted. Waiting for a worker.',
              )
            : text(
                '服务端操作已完成，请以刷新后的状态为准。',
                'Server operation completed. Read the refreshed state.',
              ),
        );
      }
    } catch (caught) {
      if (isCurrentSession(session))
        setError(
          apiErrorMessage(caught, text('实际识别操作失败', 'The recognition operation failed')),
        );
    }
  };
  if (!canRead)
    return (
      <ErrorBanner
        message={text(
          '当前账号没有敏感识别查看权限',
          'Your account cannot read sensitive recognition',
        )}
      />
    );
  return (
    <div className="space-y-5">
      <div>
        <h1 className="text-xl font-semibold">
          {text('敏感识别真实运行', 'Actual sensitive recognition runs')}
        </h1>
        <p className="mt-2 text-sm text-[var(--color-text-secondary)]">
          {text(
            '识别只读取当前租户已采集证明绑定的真实对象，最多1000行；结果只保存计数和真实发现标识。',
            'Scans read actual objects verified by this tenant’s collector, with at most 1000 rows. Results store counts and actual finding identifiers only.',
          )}
        </p>
      </div>
      {(error || tasks.error || runs.error || detail.error) && (
        <ErrorBanner
          message={
            error ||
            apiErrorMessage(
              tasks.error ?? runs.error ?? detail.error,
              text(
                '运行读取失败，旧识别计数已隐藏',
                'Runs could not be read. Previous scan counts are hidden.',
              ),
            )
          }
        />
      )}
      {notice && (
        <p role="status" className="text-sm text-[var(--color-text-secondary)]">
          {notice}
        </p>
      )}
      <section className="data-panel">
        <div className="data-toolbar flex-wrap">
          <label className="flex items-center gap-2 text-sm">
            {text('识别任务', 'Recognition task')}
            <select
              className="max-w-96 rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-primary)] px-3 py-2"
              value={taskId}
              onChange={(e) => {
                setTaskId(e.target.value);
                setOffset(0);
              }}
            >
              <option value="">
                {text('全部任务 / 选择操作任务', 'All tasks / choose a task to operate')}
              </option>
              {!tasks.error &&
                tasks.data?.data.map((task) => (
                  <option key={task.id} value={task.id}>
                    {task.name} · v{task.version} · {statusText(task.status, i18n.language)}
                  </option>
                ))}
            </select>
          </label>
          <Button
            variant="outline"
            disabled={tasks.isFetching || runs.isFetching}
            onClick={() => {
              void tasks.refetch();
              void runs.refetch();
              if (selected) void detail.refetch();
            }}
          >
            {text('刷新', 'Refresh')}
          </Button>
        </div>
        {tasks.data && tasks.data.total > 100 && (
          <p className="px-5 pb-3 text-xs text-[var(--color-text-tertiary)]">
            {text(
              '选择器仅显示前100个实际任务；更多配置可在任务配置页查看。',
              'This selector shows the first 100 actual tasks. Open task definitions for additional configurations.',
            )}
          </p>
        )}
        {task && (
          <div className="flex flex-wrap items-center gap-2 border-t border-[var(--color-border-default)] px-5 py-4">
            <span className="mr-2 text-sm">
              {task.name} · v{task.version} · {statusText(task.status, i18n.language)}
            </span>
            <Button
              variant="outline"
              size="sm"
              disabled={mutation.isPending}
              onClick={() => void act('validate')}
            >
              {text('校验定义', 'Validate definition')}
            </Button>
            {['draft', 'inactive'].includes(task.status) && canWrite && (
              <Button size="sm" disabled={mutation.isPending} onClick={() => void act('publish')}>
                {text('发布冻结版本', 'Publish frozen version')}
              </Button>
            )}
            {task.status === 'published' && (
              <>
                {canWrite && (
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={mutation.isPending}
                    onClick={() => void act('withdraw')}
                  >
                    {text('撤回任务', 'Withdraw task')}
                  </Button>
                )}
                {canExecute && (
                  <Button
                    size="sm"
                    disabled={mutation.isPending}
                    onClick={() => void act('execute')}
                  >
                    {text('执行实际识别', 'Run actual recognition')}
                  </Button>
                )}
              </>
            )}
          </div>
        )}
      </section>
      <section className="data-panel">
        <div className="data-toolbar">
          <label className="flex items-center gap-2 text-sm">
            {text('运行状态', 'Run status')}
            <select
              className="rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-primary)] p-2"
              value={status}
              onChange={(e) => {
                setStatus(e.target.value);
                setOffset(0);
              }}
            >
              <option value="">{text('全部状态', 'All states')}</option>
              {['queued', 'running', 'succeeded', 'failed', 'cancelled'].map((value) => (
                <option key={value} value={value}>
                  {statusText(value, i18n.language)}
                </option>
              ))}
            </select>
          </label>
        </div>
        <div className="overflow-auto">
          <table className="data-table">
            <thead>
              <tr>
                {[
                  text('运行标识', 'Run'),
                  text('来源版本', 'Source version'),
                  text('状态', 'Status'),
                  text('创建时间', 'Created at'),
                  text('操作', 'Actions'),
                ].map((label) => (
                  <th key={label}>{label}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {!runs.error &&
                runs.data?.data.map((run) => (
                  <tr key={run.id}>
                    <td>{run.id}</td>
                    <td>
                      {run.source_id} · v{run.source_version}
                    </td>
                    <td>{statusText(run.status, i18n.language)}</td>
                    <td>{run.created_at}</td>
                    <td>
                      <Button variant="outline" size="sm" onClick={() => choose(run.id)}>
                        {text('查看运行', 'View run')}
                      </Button>
                    </td>
                  </tr>
                ))}
              {(runs.error || !runs.data?.data.length) && (
                <tr>
                  <td colSpan={5} className="py-8 text-center text-[var(--color-text-tertiary)]">
                    {runs.isPending
                      ? text('正在读取实际运行…', 'Loading actual runs…')
                      : runs.error
                        ? text('当前运行不可读取', 'Runs are unavailable')
                        : text('当前筛选没有实际运行', 'No actual runs match these filters')}
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
        <div className="data-toolbar justify-between">
          <span className="text-sm">
            {runs.data && !runs.error
              ? text(`共 ${runs.data.total} 条运行`, `Total ${runs.data.total} runs`)
              : '—'}
          </span>
          <div className="flex gap-2">
            <Button
              variant="outline"
              disabled={offset === 0 || runs.isFetching}
              onClick={() => setOffset(Math.max(0, offset - 20))}
            >
              {text('上一页', 'Previous')}
            </Button>
            <Button
              variant="outline"
              disabled={runs.isFetching || !!runs.error || offset + 20 >= (runs.data?.total ?? 0)}
              onClick={() => setOffset(offset + 20)}
            >
              {text('下一页', 'Next')}
            </Button>
          </div>
        </div>
      </section>
      {selected && (
        <section
          className="data-panel p-5"
          aria-label={text('识别运行详情', 'Recognition run details')}
        >
          <div className="flex flex-wrap justify-between gap-2">
            <h2 className="font-semibold">{text('实际运行详情', 'Actual run details')}</h2>
            {job && canExecute && (
              <div className="flex gap-2">
                {['queued', 'running'].includes(job.status) && (
                  <Button
                    variant="outline"
                    disabled={mutation.isPending || job.cancel_requested}
                    onClick={() => void act('cancel')}
                  >
                    {job.cancel_requested
                      ? text('取消已请求', 'Cancellation requested')
                      : text('取消运行', 'Cancel run')}
                  </Button>
                )}
                {job.can_retry && ['failed', 'cancelled'].includes(job.status) && (
                  <Button disabled={mutation.isPending} onClick={() => void act('retry')}>
                    {text('按冻结版本重试', 'Retry frozen version')}
                  </Button>
                )}
              </div>
            )}
          </div>
          {job ? (
            <>
              <p className="mt-3 text-sm">
                {job.id} · {statusText(job.status, i18n.language)}
              </p>
              {(job.error || node?.error) && (
                <ErrorBanner message={job.error ?? node?.error ?? ''} />
              )}{' '}
              {result ? (
                <>
                  <SensitiveRunResult result={result} />
                  {result.finding_ids.length > 0 && (
                    <div className="mt-3 space-y-2">
                      <p className="text-sm font-medium">
                        {text('实际发现记录', 'Actual findings')}
                      </p>
                      {result.finding_ids.map((id) => (
                        <Link
                          key={id}
                          className="block break-all text-sm text-[var(--color-accent)] underline"
                          to={`/security/discovery?finding=${encodeURIComponent(id)}`}
                        >
                          {id}
                        </Link>
                      ))}
                    </div>
                  )}
                </>
              ) : (
                <p role="status" className="mt-3 text-sm text-[var(--color-text-secondary)]">
                  {job.result_access === 'restricted'
                    ? text(
                        '当前授权或来源已失效，历史计数不可查看。',
                        'Current access or source proof is invalid. Historical counts are unavailable.',
                      )
                    : ['queued', 'running'].includes(job.status)
                      ? text(
                          '实际识别尚未完成，没有可展示的成功计数。',
                          'Recognition is not finished. No successful counts are available.',
                        )
                      : text(
                          '此运行没有当前可授权的已提交识别结果。',
                          'This run has no committed scan result available under current authorization.',
                        )}
                </p>
              )}
            </>
          ) : (
            <p role="status" className="mt-3 text-sm text-[var(--color-text-secondary)]">
              {detail.isPending
                ? text(
                    '正在读取当前授权下的运行详情…',
                    'Loading run details under current authorization…',
                  )
                : text(
                    '运行详情不可读取，未展示旧计数。',
                    'Run details are unavailable. Previous counts are hidden.',
                  )}
            </p>
          )}
        </section>
      )}
    </div>
  );
}
