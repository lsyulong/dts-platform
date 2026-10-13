import { useTranslation } from 'react-i18next';
import {
  Bar,
  BarChart,
  CartesianGrid,
  Legend,
  Line,
  LineChart,
  Pie,
  PieChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';

const isRecord = (value: unknown): value is Record<string, unknown> =>
  !!value && typeof value === 'object' && !Array.isArray(value);

export default function BiChartResult({ result }: { result: Record<string, unknown> }) {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const config = isRecord(result.chart_config) ? result.chart_config : null;
  if (!config || config.type === 'table' || !['bar', 'line', 'pie'].includes(String(config.type)))
    return null;
  const x = typeof config.x_field === 'string' ? config.x_field : '';
  const y = typeof config.y_field === 'string' ? config.y_field : '';
  const rows = Array.isArray(result.rows) ? result.rows.filter(isRecord) : [];
  if (!x || !y || !rows.length) return null;
  let exact = true;
  const data = rows.map((row) => {
    const original = row[y];
    const number =
      original === null
        ? null
        : typeof original === 'number' || typeof original === 'string'
          ? Number(original)
          : NaN;
    if (
      number !== null &&
      (!Number.isFinite(number) ||
        (Number.isInteger(number) && !Number.isSafeInteger(number)) ||
        (typeof original === 'string' &&
          original.replace(/[^0-9]/g, '').replace(/^0+/, '').length > 15))
    )
      exact = false;
    return {
      category: String(row[x] ?? 'NULL'),
      value: number,
      original: original === null ? 'NULL' : String(original),
    };
  });
  if (!exact)
    return (
      <p className="mt-3 text-xs text-[var(--color-text-tertiary)]">
        {text(
          '部分度量超出图表的可靠数值范围，请使用下方原始结果表查看完整精度。',
          'Some measures exceed the chart’s reliable numeric range. Use the result table below to inspect their full precision.',
        )}
      </p>
    );
  const tooltip = (
    <Tooltip
      content={({ active, payload }) => {
        const point = payload?.[0]?.payload;
        if (!active || !isRecord(point)) return null;
        return (
          <div className="rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-3 text-xs">
            <p>{String(point.category ?? '')}</p>
            <p>
              {y}: {String(point.original ?? '')}
            </p>
          </div>
        );
      }}
    />
  );
  const legend = config.legend === true ? <Legend /> : null;
  return (
    <section
      className="mt-3 rounded-md border border-[var(--color-border-default)] p-3"
      aria-label={text('实际 BI 聚合图表', 'Actual BI aggregation chart')}
    >
      <p className="mb-3 text-sm font-medium">
        {typeof config.title === 'string' ? config.title : ''}
      </p>
      <div className="h-72 min-w-0">
        <ResponsiveContainer width="100%" height="100%">
          {config.type === 'pie' ? (
            <PieChart>
              <Pie data={data} dataKey="value" nameKey="category" fill="var(--color-accent)" />
              {tooltip}
              {legend}
            </PieChart>
          ) : config.type === 'line' ? (
            <LineChart data={data}>
              <CartesianGrid strokeDasharray="3 3" />
              <XAxis dataKey="category" />
              <YAxis />
              <Line name={y} dataKey="value" stroke="var(--color-accent)" connectNulls={false} />
              {tooltip}
              {legend}
            </LineChart>
          ) : (
            <BarChart data={data}>
              <CartesianGrid strokeDasharray="3 3" />
              <XAxis dataKey="category" />
              <YAxis />
              <Bar name={y} dataKey="value" fill="var(--color-accent)" />
              {tooltip}
              {legend}
            </BarChart>
          )}
        </ResponsiveContainer>
      </div>
    </section>
  );
}
