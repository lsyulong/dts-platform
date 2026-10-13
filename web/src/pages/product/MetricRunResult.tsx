import { useTranslation } from 'react-i18next';

export default function MetricRunResult({ result }: { result: Record<string, unknown> }) {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const window =
    result.window && typeof result.window === 'object' && !Array.isArray(result.window)
      ? (result.window as Record<string, unknown>)
      : {};
  const display = (value: unknown) =>
    value === null || value === undefined
      ? '—'
      : typeof value === 'string' || typeof value === 'number'
        ? String(value)
        : JSON.stringify(value);
  return (
    <div className="rounded-md bg-[var(--color-bg-secondary)] p-4 space-y-2 text-sm">
      <p className="font-medium">
        {text('实际指标计算', 'Observed metric calculation')} · v{display(result.metric_version)}
      </p>
      <p>
        {text('数据窗口', 'Data window')}: {display(window.start)} ≤ t &lt; {display(window.end)} ·{' '}
        {display(window.timezone)}
      </p>
      <p>
        {text('物理模型版本', 'Physical model version')}: {display(result.model_version)} ·{' '}
        {text('元数据版本', 'Metadata version')}: {display(result.metadata_version)}
      </p>
      <p>
        {text('实际粒度字段', 'Actual grain columns')}: {display(result.grain_columns)}
      </p>
      <p className="text-[var(--color-text-secondary)]">
        {text(
          'input_rows 是窗口内的实际输入行数。NULL 指标值表示结果未定义；没有数据不等于业务值为零。',
          'input_rows is the actual input count in this window. NULL denotes an undefined result. Empty data does not imply a zero business value.',
        )}
      </p>
      {result.numeric_semantics !== undefined && (
        <p>
          {text('数值语义', 'Numeric semantics')}: {display(result.numeric_semantics)}
        </p>
      )}
    </div>
  );
}
