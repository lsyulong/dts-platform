import { useState } from 'react';
import { Link, useSearchParams } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { Button } from '../../components/ui/button';
import { ErrorBanner } from '../../components/ui';
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '../../components/ui/dialog';
import { apiErrorMessage } from '../../lib/apiError';
import { captureSession, isCurrentSession } from '../../lib/session';
import { useAuthStore } from '../../stores/auth';
import type { ProductModule } from '../../product/catalog-types';
import {
  useSensitiveFindings,
  useSensitiveFinding,
  useSensitiveReview,
  type SensitiveFinding,
} from './sensitive-api';

export default function SensitiveFindingsPage({
  taskId,
}: { module?: ProductModule; taskId?: string } = {}) {
  const identity = useAuthStore((state) =>
    JSON.stringify([state.user?.tenant_id, state.user?.id, state.token, state.user?.permissions]),
  );
  return <Findings key={`${identity}:${taskId ?? ''}`} taskId={taskId} />;
}
function Findings({ taskId }: { taskId?: string }) {
  const { i18n } = useTranslation(),
    text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const user = useAuthStore((state) => state.user),
    canRead = !!user?.permissions.includes('sensitive_data:read'),
    canWrite = !!user?.permissions.includes('sensitive_data:write');
  const [params, setParams] = useSearchParams(),
    [status, setStatus] = useState(''),
    [offset, setOffset] = useState(0),
    [error, setError] = useState('');
  const [pending, setPending] = useState<{
      finding: SensitiveFinding;
      action: 'confirm' | 'dismiss';
    } | null>(null),
    [reason, setReason] = useState('');
  const chosenTask = taskId ?? params.get('task') ?? undefined,
    selected = params.get('finding') ?? undefined;
  const list = useSensitiveFindings(chosenTask, status, offset),
    detail = useSensitiveFinding(selected),
    review = useSensitiveReview();
  const reviewState = (value: SensitiveFinding['status']) =>
    value === 'pending'
      ? text('待复核', 'Pending review')
      : value === 'confirmed'
        ? text('已确认', 'Confirmed')
        : text('已忽略', 'Dismissed');
  const records =
    canRead && !list.error && (!selected || !detail.error) ? (list.data?.data ?? []) : [];
  const latest = records.find((row) => row.id === selected),
    candidate = detail.data;
  const found =
    !!selected &&
    !!candidate &&
    canRead &&
    !list.error &&
    !detail.error &&
    candidate?.id === selected &&
    candidate.tenant_id === user?.tenant_id &&
    (!latest || latest.version === candidate.version)
      ? candidate
      : null;
  const open = (id: string) => {
    const next = new URLSearchParams(params);
    next.set('finding', id);
    setParams(next);
  };
  const close = () => {
    const next = new URLSearchParams(params);
    next.delete('finding');
    setParams(next);
  };
  const decide = async () => {
    if (
      !pending ||
      review.isPending ||
      !canWrite ||
      !reason.trim() ||
      new TextEncoder().encode(reason).length > 2000
    )
      return;
    const session = captureSession();
    setError('');
    try {
      const result = await review.mutateAsync({
        id: pending.finding.id,
        action: pending.action,
        expected_version: pending.finding.version,
        reason,
      });
      if (isCurrentSession(session)) {
        if (
          result.reply.id !== pending.finding.id ||
          result.reply.status !== (pending.action === 'confirm' ? 'confirmed' : 'dismissed') ||
          result.reply.version <= pending.finding.version
        )
          throw new Error(
            text(
              '复核回执与实际发现不匹配，请刷新',
              'Review receipt does not match the finding. Refresh first.',
            ),
          );
        setPending(null);
        setReason('');
      }
    } catch (caught) {
      if (isCurrentSession(session))
        setError(
          apiErrorMessage(
            caught,
            text('复核失败，未确认发现', 'Review failed. The finding was not confirmed.'),
          ),
        );
    }
  };
  const begin = (finding: SensitiveFinding, action: 'confirm' | 'dismiss') => {
    setPending({ finding, action });
    setReason('');
    setError('');
  };
  if (!canRead)
    return (
      <ErrorBanner
        message={text(
          '当前账号没有敏感识别查看权限',
          'Your account cannot read sensitive recognition evidence',
        )}
      />
    );
  return (
    <div className="space-y-5">
      <div>
        <h1 className="text-xl font-semibold">
          {text('敏感数据发现与复核', 'Sensitive data findings and review')}
        </h1>
        <p className="mt-2 text-sm text-[var(--color-text-secondary)]">
          {text(
            '只读取真实扫描产生的计数证据。抽样未匹配不代表整表安全，人工确认不会自动配置脱敏或授予数据权限。',
            'Only evidence produced by actual scans is shown. No sample matches do not certify the table as safe. Review does not configure masking or grant access.',
          )}
        </p>
      </div>
      {((error && !pending) || list.error || detail.error) && (
        <ErrorBanner
          message={
            (!pending ? error : '') ||
            apiErrorMessage(
              list.error ?? detail.error,
              text(
                '证据读取失败，旧计数已隐藏',
                'Evidence could not be read. Previous counts are hidden.',
              ),
            )
          }
        />
      )}
      <section className="data-panel">
        <div className="data-toolbar flex-wrap">
          <label className="flex items-center gap-2 text-sm">
            {text('复核状态', 'Review status')}
            <select
              className="rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-primary)] px-3 py-2"
              value={status}
              onChange={(e) => {
                setStatus(e.target.value);
                setOffset(0);
                close();
              }}
            >
              <option value="">{text('全部状态', 'All states')}</option>
              {(['pending', 'confirmed', 'dismissed'] as const).map((value) => (
                <option key={value} value={value}>
                  {reviewState(value)}
                </option>
              ))}
            </select>
          </label>
          <Button
            variant="outline"
            disabled={list.isFetching}
            onClick={() => {
              void list.refetch();
              if (selected) void detail.refetch();
            }}
          >
            {text('刷新', 'Refresh')}
          </Button>
          {chosenTask && (
            <span className="text-xs text-[var(--color-text-tertiary)]">
              {text('任务筛选', 'Task filter')}: {chosenTask}
            </span>
          )}
        </div>
        <div className="overflow-auto">
          <table className="data-table">
            <thead>
              <tr>
                {[
                  text('字段 / 来源', 'Column / source'),
                  text('范围', 'Scope'),
                  text('匹配 / 检测 / NULL', 'Matches / checked / NULL'),
                  text('建议密级 / 版本', 'Proposed classification / version'),
                  text('状态', 'Status'),
                  text('操作', 'Actions'),
                ].map((label) => (
                  <th key={label}>{label}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {records.map((finding) => (
                <tr key={finding.id}>
                  <td>
                    <strong>{finding.column_name}</strong>
                    <p className="mt-1 text-xs text-[var(--color-text-tertiary)]">
                      {finding.object_id} · v{finding.object_version}
                    </p>
                  </td>
                  <td>
                    {finding.evaluation_scope === 'sample'
                      ? text('抽样', 'Sample')
                      : text('字段名称', 'Column names')}
                    <p className="text-xs">
                      {finding.evaluation_scope === 'sample'
                        ? `${finding.sampled_rows} ${text('实际行', 'observed rows')}`
                        : text('未扫描数据值', 'No values scanned')}
                    </p>
                  </td>
                  <td>
                    {finding.evidence.matched_values} / {finding.evidence.checked_values} /{' '}
                    {finding.evidence.null_values}
                  </td>
                  <td>
                    {finding.classification_id} · v{finding.classification_version}
                  </td>
                  <td>{reviewState(finding.status)}</td>
                  <td>
                    <div className="flex flex-wrap gap-2">
                      <Button variant="outline" size="sm" onClick={() => open(finding.id)}>
                        {text('查看证据', 'View evidence')}
                      </Button>
                      {finding.status === 'pending' && canWrite && (
                        <>
                          <Button size="sm" onClick={() => begin(finding, 'confirm')}>
                            {text('确认建议', 'Confirm proposal')}
                          </Button>
                          <Button
                            size="sm"
                            variant="outline"
                            onClick={() => begin(finding, 'dismiss')}
                          >
                            {text('忽略发现', 'Dismiss finding')}
                          </Button>
                        </>
                      )}
                    </div>
                  </td>
                </tr>
              ))}
              {!records.length && (
                <tr>
                  <td colSpan={6} className="py-8 text-center text-[var(--color-text-tertiary)]">
                    {list.isPending
                      ? text('正在读取实际发现…', 'Loading actual findings…')
                      : list.error
                        ? text(
                            '证据访问受限或服务不可用',
                            'Evidence access is restricted or unavailable',
                          )
                        : text(
                            '当前筛选没有真实发现记录',
                            'No actual findings match the current filters',
                          )}
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
        <div className="data-toolbar justify-between">
          <span className="text-sm">
            {!list.error && list.data
              ? text(`共 ${list.data.total} 条记录`, `Total ${list.data.total} records`)
              : '—'}
          </span>
          <div className="flex gap-2">
            <Button
              variant="outline"
              disabled={offset === 0 || list.isFetching}
              onClick={() => {
                setOffset(Math.max(0, offset - 20));
                close();
              }}
            >
              {text('上一页', 'Previous')}
            </Button>
            <Button
              variant="outline"
              disabled={list.isFetching || !!list.error || offset + 20 >= (list.data?.total ?? 0)}
              onClick={() => {
                setOffset(offset + 20);
                close();
              }}
            >
              {text('下一页', 'Next')}
            </Button>
          </div>
        </div>
      </section>
      {selected && (
        <section
          className="data-panel p-5"
          aria-label={text('发现证据详情', 'Finding evidence details')}
        >
          <div className="flex justify-between">
            <h2 className="font-semibold">{text('真实发现证据', 'Actual finding evidence')}</h2>
            <Button variant="ghost" size="sm" onClick={close}>
              {text('关闭详情', 'Close details')}
            </Button>
          </div>
          {found ? (
            <div className="mt-4 space-y-3 text-sm">
              <dl className="grid gap-3 sm:grid-cols-2">
                <div>
                  <dt>{text('发现标识 / 版本', 'Finding / version')}</dt>
                  <dd>
                    {found.id} · v{found.version}
                  </dd>
                </div>
                <div>
                  <dt>{text('实际运行 / 节点', 'Actual job / node')}</dt>
                  <dd>
                    {found.job_id} / {found.node_id}
                  </dd>
                </div>
                <div>
                  <dt>{text('字段 / 实际类型', 'Column / actual type')}</dt>
                  <dd>
                    {found.column_name} / {found.evidence.data_type}
                  </dd>
                </div>
                <div>
                  <dt>{text('匹配值 / 检测值 / NULL', 'Matched / checked / NULL')}</dt>
                  <dd>
                    {found.evidence.matched_values} / {found.evidence.checked_values} /{' '}
                    {found.evidence.null_values}
                  </dd>
                </div>
              </dl>
              <p className="break-all text-xs text-[var(--color-text-tertiary)]">
                {text('冻结定义摘要', 'Frozen definition hash')}: {found.definition_hash}
              </p>
              {found.review_reason && (
                <p>
                  {text('复核意见', 'Review reason')}: {found.review_reason} · {found.reviewed_by}
                </p>
              )}
              <Link
                className="text-[var(--color-accent)] underline"
                to={`/security/recognition-tasks?job=${encodeURIComponent(found.job_id)}`}
              >
                {text('查看实际运行来源', 'View actual source run')}
              </Link>
            </div>
          ) : (
            <p role="status" className="mt-4 text-sm text-[var(--color-text-secondary)]">
              {detail.isPending
                ? text('正在校验当前证据访问权限…', 'Checking current evidence access…')
                : text(
                    '当前证据不可读取，未展示缓存计数。',
                    'This evidence is unavailable. Cached counts are hidden.',
                  )}
            </p>
          )}
        </section>
      )}
      <Dialog
        open={!!pending}
        onOpenChange={(open) => {
          if (!open && !review.isPending) setPending(null);
        }}
      >
        <DialogContent>
          <DialogTitle>
            {pending?.action === 'confirm'
              ? text('确认分类建议', 'Confirm classification proposal')
              : text('忽略敏感发现', 'Dismiss sensitive finding')}
          </DialogTitle>
          <DialogDescription>
            {text(
              '这次复核只更新发现状态。确认必须重验当前定义和来源，忽略保留真实历史记录和审计。',
              'Review updates the finding state. Confirmation rechecks current definitions and source proof; dismissal preserves the actual history and audit.',
            )}
          </DialogDescription>
          <p className="text-sm">
            {pending?.finding.column_name} · v{pending?.finding.version}
          </p>
          <label className="space-y-2 text-sm">
            <span>{text('复核意见（必填）', 'Review reason (required)')}</span>
            <textarea
              className="block min-h-28 w-full rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-primary)] p-3"
              value={reason}
              onChange={(e) => setReason(e.target.value)}
              disabled={review.isPending}
              maxLength={2000}
            />
          </label>
          {error && <ErrorBanner message={error} />}
          <div className="flex justify-end gap-2">
            <Button variant="outline" disabled={review.isPending} onClick={() => setPending(null)}>
              {text('取消', 'Cancel')}
            </Button>
            <Button
              disabled={
                review.isPending ||
                !reason.trim() ||
                new TextEncoder().encode(reason).length > 2000 ||
                !canWrite ||
                !!list.error
              }
              onClick={() => void decide()}
            >
              {review.isPending
                ? text('提交真实复核…', 'Submitting review…')
                : text('提交复核', 'Submit review')}
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}
