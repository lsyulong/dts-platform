import { useState, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import client from '../../api/client';
import { Button } from '../../components/ui/button';
import { ErrorBanner } from '../../components/ui';
import ReferencePicker from '../../product/ReferencePicker';
import { apiErrorMessage } from '../../lib/apiError';
import { captureSession, isCurrentSession } from '../../lib/session';
import { useAuthStore } from '../../stores/auth';

interface QualityReport {
  status: string;
  completed_runs: number;
  failed_runs: number;
  incomplete_runs: number;
  checked_rows: number;
  failed_rows: number;
  failure_ratio: number | null;
  counting_unit: string;
  window_complete: boolean;
  empty_checks: number;
  sampled_checks: number;
  jobs_scanned: number;
  total_jobs: number;
}

const reportStatuses: Record<string, [string, string]> = {
  no_results: ['暂无校验结果', 'No check results'],
  empty_data: ['没有可校验数据', 'No data to check'],
  partial_empty_data: ['部分校验没有数据', 'Some checks have no data'],
  incomplete_window: ['历史扫描不完整', 'Incomplete history window'],
  incomplete_runs: ['存在未完成运行', 'Some runs are incomplete'],
  execution_error: ['执行异常', 'Execution error'],
  threshold_exceeded: ['超过失败阈值', 'Failure threshold exceeded'],
  within_threshold: ['在失败阈值以内', 'Within failure threshold'],
  sample_within_threshold: ['抽样在失败阈值以内', 'Sample within failure threshold'],
  sample_passed: ['抽样校验通过', 'Sample checks passed'],
  passed: ['校验通过', 'Checks passed'],
  failed: ['校验失败', 'Checks failed'],
};

export default function QualityReportPage() {
  const identity = useAuthStore((state) =>
    JSON.stringify([state.user?.tenant_id, state.user?.id, state.token, state.user?.permissions]),
  );
  return <QualityReportContent key={identity} />;
}

function QualityReportContent() {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const canRead = useAuthStore((state) => state.user?.permissions.includes('quality:read'));
  const [taskId, setTaskId] = useState('');
  const [start, setStart] = useState(() =>
    new Date(Date.now() - 7 * 86400000).toISOString().slice(0, 10),
  );
  const [end, setEnd] = useState(() => new Date().toISOString().slice(0, 10));
  const [generated, setGenerated] = useState<{
    data: QualityReport;
    start: string;
    end: string;
  } | null>(null);
  const report = generated?.data;
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const generate = async (event: FormEvent) => {
    event.preventDefault();
    if (busy || !canRead) return;
    const session = captureSession();
    setBusy(true);
    setError('');
    setGenerated(null);
    try {
      const periodEnd = new Date(`${end}T00:00:00`);
      periodEnd.setDate(periodEnd.getDate() + 1);
      const response = await client.post<QualityReport>(
        '/product/quality/report',
        {
          task_id: taskId || null,
          period_start: new Date(`${start}T00:00:00`).toISOString(),
          period_end: periodEnd.toISOString(),
        },
        { headers: { Authorization: `Bearer ${session.token}` } },
      );
      if (isCurrentSession(session)) setGenerated({ data: response.data, start, end });
    } catch (caught) {
      if (isCurrentSession(session))
        setError(apiErrorMessage(caught, text('报告生成失败', 'Could not generate report')));
    } finally {
      if (isCurrentSession(session)) setBusy(false);
    }
  };
  const exportReport = () => {
    if (!report || !generated || !canRead) return;
    const url = URL.createObjectURL(
      new Blob([JSON.stringify(report, null, 2)], { type: 'application/json;charset=utf-8' }),
    );
    const link = document.createElement('a');
    link.href = url;
    link.download = `quality-report-${generated.start}-${generated.end}.json`;
    link.click();
    URL.revokeObjectURL(url);
  };
  if (!canRead)
    return (
      <ErrorBanner
        message={text('当前账号没有查看权限', 'Your account cannot read quality reports')}
      />
    );
  return (
    <div className="space-y-5">
      <div>
        <h1 className="text-xl font-semibold">{text('质量报告', 'Quality reports')}</h1>
        <p className="mt-1 text-sm text-[var(--color-text-tertiary)]">
          {text(
            '按实际执行结果生成时间范围内的质量报告',
            'Generate a quality report from observed execution results',
          )}
        </p>
      </div>
      <section className="data-panel p-5">
        <form
          className="grid items-end gap-4 sm:grid-cols-4"
          onSubmit={(event) => void generate(event)}
        >
          <ReferencePicker
            kind="quality_task"
            label={text('质量任务（可选）', 'Quality task (optional)')}
            value={taskId}
            onChange={setTaskId}
            disabled={busy || !canRead}
          />
          <label className="text-sm">
            {text('开始日期', 'Start date')}
            <input
              className="mt-2 block w-full rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-2"
              type="date"
              required
              value={start}
              max={end}
              onChange={(event) => setStart(event.target.value)}
              disabled={busy}
            />
          </label>
          <label className="text-sm">
            {text('结束日期', 'End date')}
            <input
              className="mt-2 block w-full rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-2"
              type="date"
              required
              value={end}
              min={start}
              onChange={(event) => setEnd(event.target.value)}
              disabled={busy}
            />
          </label>
          <Button type="submit" disabled={busy || !canRead}>
            {busy ? text('正在生成', 'Generating') : text('生成报告', 'Generate report')}
          </Button>
        </form>
        {error && (
          <div className="mt-4">
            <ErrorBanner message={error} />
          </div>
        )}
      </section>
      {report ? (
        <section className="data-panel p-5">
          <div className="flex items-center justify-between">
            <h2 className="font-medium">
              {text('报告结果', 'Report result')} ·{' '}
              {reportStatuses[report.status]?.[i18n.language.startsWith('zh') ? 0 : 1] ??
                report.status}
            </h2>
            <Button variant="outline" onClick={exportReport}>
              {text('导出报告', 'Export report')}
            </Button>
          </div>
          <p className="mt-2 text-xs text-[var(--color-text-tertiary)]">
            {generated?.start} — {generated?.end}
          </p>
          {['no_results', 'empty_data', 'partial_empty_data'].includes(report.status) && (
            <p className="mt-4 text-sm">
              {text(
                '缺少实际校验数据，不能据此判定质量通过。',
                'Missing check data cannot establish a passing quality status.',
              )}
            </p>
          )}
          {!report.window_complete && (
            <div className="mt-4">
              <ErrorBanner
                message={text(
                  '历史数据超过本次扫描范围，报告不完整，不能据此判定质量通过。',
                  'History exceeds the scanned range. This incomplete report cannot establish a passing quality status.',
                )}
              />
            </div>
          )}
          <dl className="mt-5 grid gap-4 sm:grid-cols-3">
            {[
              [text('完成运行', 'Completed runs'), report.completed_runs],
              [text('失败运行', 'Failed runs'), report.failed_runs],
              [text('未完成运行', 'Incomplete runs'), report.incomplete_runs],
              [text('行与规则组合校验数', 'Row-rule checks'), report.checked_rows],
              [text('失败校验数', 'Failed checks'), report.failed_rows],
              [
                text('失败比例', 'Failure ratio'),
                report.failure_ratio === null ? '—' : `${(report.failure_ratio * 100).toFixed(2)}%`,
              ],
              [text('空数据校验', 'Empty checks'), report.empty_checks],
              [text('采用抽样的校验', 'Sampled checks'), report.sampled_checks],
              [text('扫描运行数', 'Scanned runs'), `${report.jobs_scanned} / ${report.total_jobs}`],
            ].map(([label, value]) => (
              <div key={label} className="rounded-md bg-[var(--color-bg-secondary)] p-4">
                <dt className="text-xs text-[var(--color-text-tertiary)]">{label}</dt>
                <dd className="mt-2 text-xl font-semibold">{value}</dd>
              </div>
            ))}
          </dl>
        </section>
      ) : (
        <div className="py-14 text-center text-sm text-[var(--color-text-tertiary)]">
          {text(
            '选择范围后生成报告；没有校验结果时会明确显示暂无数据。',
            'Choose a range to generate a report. Missing results are reported as no data.',
          )}
        </div>
      )}
    </div>
  );
}
