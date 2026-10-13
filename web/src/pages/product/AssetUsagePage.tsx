import { useState, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import client from '../../api/client';
import { Button } from '../../components/ui/button';
import { ErrorBanner, FormInput } from '../../components/ui';
import ReferencePicker from '../../product/ReferencePicker';
import { useAuthStore } from '../../stores/auth';
import { captureSession, isCurrentSession } from '../../lib/session';
import { apiErrorMessage } from '../../lib/apiError';
import AssetUsageResult, { type AssetUsage } from './AssetUsageResult';

const localTime = (time: number) => {
  const date = new Date(time);
  return new Date(time - date.getTimezoneOffset() * 60000).toISOString().slice(0, 16);
};

export default function AssetUsagePage() {
  const identity = useAuthStore((state) =>
    JSON.stringify([state.user?.tenant_id, state.user?.id, state.token, state.user?.permissions]),
  );
  return <AssetUsageContent key={identity} />;
}

function AssetUsageContent() {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const canRead = useAuthStore((state) => state.user?.permissions.includes('assets:read'));
  const [assetId, setAssetId] = useState('');
  const [start, setStart] = useState(() => localTime(Date.now() - 7 * 86400000));
  const [end, setEnd] = useState(() => localTime(Date.now()));
  const [report, setReport] = useState<AssetUsage | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const analyze = async (event: FormEvent) => {
    event.preventDefault();
    if (!canRead || busy) return;
    const session = captureSession();
    setReport(null);
    setError('');
    setBusy(true);
    try {
      const from = new Date(start),
        to = new Date(end);
      if (
        !assetId ||
        !Number.isFinite(from.getTime()) ||
        !Number.isFinite(to.getTime()) ||
        to <= from
      )
        throw new Error(
          text('请选择资产并填写有效的起止时间', 'Select an asset and a valid time window'),
        );
      const response = await client.post<AssetUsage>(
        '/product/assets/usage',
        { asset_id: assetId, period_start: from.toISOString(), period_end: to.toISOString() },
        { headers: { Authorization: `Bearer ${session.token}` } },
      );
      if (isCurrentSession(session)) setReport(response.data);
    } catch (caught) {
      if (isCurrentSession(session))
        setError(apiErrorMessage(caught, text('资产分析失败', 'Asset analysis failed')));
    } finally {
      if (isCurrentSession(session)) setBusy(false);
    }
  };
  const download = () => {
    if (!report || !canRead) return;
    const url = URL.createObjectURL(
      new Blob([JSON.stringify(report, null, 2)], { type: 'application/json;charset=utf-8' }),
    );
    const link = document.createElement('a');
    link.href = url;
    link.download = `asset-usage-${report.asset_id}.json`;
    link.click();
    URL.revokeObjectURL(url);
  };
  if (!canRead)
    return (
      <ErrorBanner message={text('当前账号没有资产查看权限', 'Your account cannot read assets')} />
    );
  return (
    <div className="space-y-5">
      <div>
        <h1 className="text-xl font-semibold">{text('资产分析', 'Asset analysis')}</h1>
        <p className="mt-1 text-sm text-[var(--color-text-tertiary)]">
          {text(
            '按时间范围分析已记录的受控查询访问事件',
            'Analyze recorded controlled-query access events in a time window',
          )}
        </p>
      </div>
      <section className="data-panel p-5">
        <form
          onSubmit={(event) => void analyze(event)}
          className="grid items-end gap-4 sm:grid-cols-2 xl:grid-cols-4"
        >
          <ReferencePicker
            kind="asset_entry"
            label={text('分析资产', 'Asset')}
            value={assetId}
            onChange={(value) => {
              setAssetId(value);
              setReport(null);
            }}
            required
            disabled={busy}
          />
          <FormInput
            label={text('开始时间（包含）', 'Start time (inclusive)')}
            type="datetime-local"
            value={start}
            onChange={(value) => {
              setStart(value);
              setReport(null);
            }}
            required
            disabled={busy}
          />
          <FormInput
            label={text('结束时间（不含）', 'End time (exclusive)')}
            type="datetime-local"
            value={end}
            onChange={(value) => {
              setEnd(value);
              setReport(null);
            }}
            required
            disabled={busy}
          />
          <Button type="submit" disabled={busy || !assetId}>
            {busy ? text('正在分析…', 'Analyzing…') : text('分析使用情况', 'Analyze usage')}
          </Button>
        </form>
      </section>
      {error && <ErrorBanner message={error} />}
      {report && (
        <>
          <AssetUsageResult report={report} />
          <div className="flex justify-end">
            <Button variant="outline" onClick={download}>
              {text('导出本次分析', 'Export this analysis')}
            </Button>
          </div>
        </>
      )}
    </div>
  );
}
