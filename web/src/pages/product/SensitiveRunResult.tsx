import { useTranslation } from 'react-i18next';

export interface SensitiveScanResult {
  evaluation_scope: 'sample' | 'column_names';
  sample_limit: number;
  sampled_rows: number;
  columns: {
    column_name: string;
    data_type: string;
    checked_values: number;
    matched_values: number;
    null_values: number;
  }[];
  definition_hash: string;
  producer_committed: boolean;
  finding_ids: string[];
}
export default function SensitiveRunResult({ result }: { result: SensitiveScanResult }) {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const sampled = result.evaluation_scope === 'sample';
  return (
    <div className="mt-3 space-y-4">
      <div className="rounded-md bg-[var(--color-bg-secondary)] p-4 text-sm">
        <p className="font-medium">
          {sampled
            ? text('实际抽样识别', 'Observed sample recognition')
            : text('实际字段名称识别', 'Actual column name recognition')}
        </p>
        <p className="mt-2 text-[var(--color-text-secondary)]">
          {sampled
            ? text(
                '本次结果只代表实际返回的样本；未匹配不代表整表不存在敏感数据。',
                'These results describe only the returned sample. No matches do not imply the entire table has no sensitive data.',
              )
            : text(
                '按实际采集的字段名称匹配规则，没有扫描字段值。',
                'Rules were matched against actual column names. Field values were not scanned.',
              )}
        </p>
        <dl className="mt-3 flex flex-wrap gap-6">
          {sampled && (
            <div>
              <dt>{text('实际抽样行数 / 上限', 'Observed rows / limit')}</dt>
              <dd className="font-semibold">
                {result.sampled_rows} / {result.sample_limit}
              </dd>
            </div>
          )}
          <div>
            <dt>{text('检测字段数', 'Checked columns')}</dt>
            <dd className="font-semibold">{result.columns.length}</dd>
          </div>
          <div>
            <dt>{text('持久化发现数', 'Persisted findings')}</dt>
            <dd className="font-semibold">
              {result.producer_committed ? result.finding_ids.length : '—'}
            </dd>
          </div>
        </dl>
        {!result.producer_committed && (
          <p role="status" className="mt-3 text-amber-600">
            {text(
              '结果尚未获得发现记录的提交回执。',
              'No committed finding receipt is available yet.',
            )}
          </p>
        )}
      </div>
      <div className="overflow-auto rounded-md border border-[var(--color-border-default)]">
        <table className="data-table">
          <thead>
            <tr>
              {[
                text('字段', 'Column'),
                text('实际类型', 'Actual type'),
                text('检测值', 'Checked values'),
                text('匹配值', 'Matched values'),
                text('NULL 值', 'NULL values'),
              ].map((label) => (
                <th key={label}>{label}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {result.columns.map((column) => (
              <tr key={column.column_name}>
                <td>{column.column_name}</td>
                <td>{column.data_type}</td>
                <td>{column.checked_values}</td>
                <td>{column.matched_values}</td>
                <td>{column.null_values}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <p className="break-all text-xs text-[var(--color-text-tertiary)]">
        {text('冻结定义摘要', 'Frozen definition hash')}: {result.definition_hash}
      </p>
    </div>
  );
}
