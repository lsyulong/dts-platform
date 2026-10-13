import { useTranslation } from 'react-i18next';

export interface AssetUsage {
  asset_id: string;
  metadata_id: string | null;
  period_start: string;
  period_end: string;
  instrumentation: string;
  scope: string;
  complete: boolean;
  limit: number;
  matched_recorded_events: number | null;
  counted_recorded_events: number;
  succeeded: number | null;
  failed: number | null;
  denied: number | null;
  unique_actors: number | null;
}

export default function AssetUsageResult({ report }: { report: AssetUsage }) {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const recorded = report.instrumentation === 'data_access_audit';
  const metrics = [
    [text('记录的访问事件', 'Recorded access events'), report.matched_recorded_events],
    [text('成功访问', 'Successful access'), report.succeeded],
    [text('查询失败', 'Failed queries'), report.failed],
    [text('拒绝访问', 'Denied access'), report.denied],
    [text('访问用户', 'Distinct users'), report.unique_actors],
  ] as const;
  return (
    <section
      className="data-panel space-y-4 p-5"
      aria-label={text('资产使用分析结果', 'Asset usage analysis results')}
    >
      <div className="text-sm text-[var(--color-text-secondary)]">
        {new Date(report.period_start).toLocaleString(i18n.language)} —{' '}
        {new Date(report.period_end).toLocaleString(i18n.language)}
      </div>
      {!recorded ? (
        <p role="status" className="rounded-md bg-[var(--color-bg-secondary)] p-3 text-sm">
          {text(
            '该类资产尚未记录使用事件，统计值暂不可用。',
            'Usage events are not recorded for this asset type; statistics are unavailable.',
          )}
        </p>
      ) : !report.complete ? (
        <p role="status" className="rounded-md bg-amber-500/10 p-3 text-sm">
          {text(
            '事件数量超过本次统计上限，请缩短时间范围；完整统计值暂不可用。',
            'The event count exceeds the analysis limit. Shorten the window to obtain complete statistics.',
          )}{' '}
          {text('上限', 'Limit')}: {report.limit}
        </p>
      ) : (
        <p className="text-sm text-[var(--color-accent)]">
          {text(
            '已统计该时间范围内记录的访问事件。',
            'Recorded access events in this window have been counted.',
          )}
        </p>
      )}
      <dl className="grid gap-3 sm:grid-cols-3 xl:grid-cols-5">
        {metrics.map(([label, value]) => (
          <div key={label} className="rounded-md border border-[var(--color-border-default)] p-4">
            <dt className="text-xs text-[var(--color-text-tertiary)]">{label}</dt>
            <dd className="mt-2 text-xl font-semibold">
              {value === null ? '—' : value.toLocaleString(i18n.language)}
            </dd>
          </div>
        ))}
      </dl>
      <p className="text-xs text-[var(--color-text-tertiary)]">
        {text(
          '仅统计审计证据明确关联此元数据 ID 的受控查询事件。未关联对象的审计、外部工具和直接数据库访问不计入该统计。',
          'Only controlled-query audit events explicitly linked to this metadata ID are counted. Unlinked audits, external tools and direct database access are outside this count.',
        )}
      </p>
    </section>
  );
}
