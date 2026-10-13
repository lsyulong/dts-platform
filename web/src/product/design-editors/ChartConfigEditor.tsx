import { BarChart3, Circle, Table2 } from 'lucide-react';
import {
  chartConfig,
  chartTypes,
  reportDimensions,
  reportMeasures,
  validateChartConfig,
  type ChartConfig,
  type DesignEditorProps,
} from './design-models';
import { DesignHeader, DesignWarning } from './design-ui';
import { designField, designInput, designPanel, useDesignText } from './design-text';

function FieldSelector({
  label,
  value,
  options,
  required,
  onChange,
}: {
  label: string;
  value: string;
  options: string[];
  required: boolean;
  onChange: (value: string) => void;
}) {
  const text = useDesignText();
  return (
    <label className={designField}>
      <span>{label}</span>
      {options.length ? (
        <select
          className={designInput}
          value={value}
          required={required}
          onChange={(event) => onChange(event.target.value)}
        >
          <option value="">{text('请选择字段', 'Choose a field')}</option>
          {value && !options.includes(value) && (
            <option value={value}>
              {text('未声明字段', 'Undeclared field')}: {value}
            </option>
          )}
          {options.map((name) => (
            <option key={name} value={name}>
              {name}
            </option>
          ))}
        </select>
      ) : (
        <input
          className={designInput}
          value={value}
          required={required}
          maxLength={128}
          placeholder={text('填写实际查询字段名称', 'Enter an actual query field name')}
          onChange={(event) => onChange(event.target.value)}
        />
      )}
    </label>
  );
}

export default function ChartConfigEditor({
  value,
  onChange,
  disabled,
  properties,
}: DesignEditorProps) {
  const text = useDesignText();
  const config = chartConfig(value);
  const dimensions = [
    ...new Set(
      reportDimensions(properties?.dimensions)
        .map((value) => value.trim())
        .filter(Boolean),
    ),
  ];
  const measures = [
    ...new Set(
      reportMeasures(properties?.measures)
        .map((value) => value.name.trim())
        .filter(Boolean),
    ),
  ];
  const update = (patch: Partial<ChartConfig>) => onChange({ ...config, ...patch });
  const errors = value === undefined ? [] : validateChartConfig(value, properties);
  const typeLabel = (type: string) =>
    ({
      bar: text('柱状图', 'Bar chart'),
      line: text('折线图', 'Line chart'),
      pie: text('饼图', 'Pie chart'),
      table: text('表格', 'Table'),
    })[type];
  const previewHeaders = [
    ...new Set([...dimensions, ...measures, config.x_field, config.y_field].filter(Boolean)),
  ];

  return (
    <fieldset className={designPanel} disabled={disabled}>
      <legend className="sr-only">{text('BI 图表配置', 'BI chart configuration')}</legend>
      <DesignHeader title={text('BI 图表配置', 'BI chart configuration')} />
      <div className="grid gap-3 sm:grid-cols-2">
        <label className={designField}>
          <span>{text('图表类型', 'Chart type')}</span>
          <select
            className={designInput}
            value={config.type}
            onChange={(event) => {
              const type = chartTypes.find((type) => type === event.target.value);
              if (type) update({ type });
            }}
          >
            {chartTypes.map((type) => (
              <option key={type} value={type}>
                {typeLabel(type)}
              </option>
            ))}
          </select>
        </label>
        <label className={designField}>
          <span>{text('图表标题', 'Chart title')}</span>
          <input
            className={designInput}
            value={config.title}
            required
            maxLength={120}
            onChange={(event) => update({ title: event.target.value })}
          />
        </label>
        <FieldSelector
          label={text('分类 / X 轴字段', 'Category / X-axis field')}
          value={config.x_field}
          options={dimensions}
          required={config.type !== 'table'}
          onChange={(x_field) => update({ x_field })}
        />
        <FieldSelector
          label={text('度量 / Y 轴字段', 'Measure / Y-axis field')}
          value={config.y_field}
          options={measures}
          required={config.type !== 'table'}
          onChange={(y_field) => update({ y_field })}
        />
        <label className="flex items-center gap-2 text-sm text-[var(--color-text-secondary)]">
          <input
            type="checkbox"
            checked={config.legend}
            onChange={(event) => update({ legend: event.target.checked })}
          />
          {text('显示图例', 'Show legend')}
        </label>
      </div>
      <p className="mt-3 text-xs leading-5 text-[var(--color-text-tertiary)]">
        {text(
          '优先选择已配置的维度和度量。尚未配置时，可填写实际 SQL 返回字段；执行时仍需核对字段与类型。',
          'Choose configured dimensions and measures when available. Otherwise enter actual SQL result fields; execution must still verify their names and types.',
        )}
      </p>
      {!!errors.length && (
        <DesignWarning>
          {text(
            '请检查图表标题、类型、图例设置和所需字段；声明的维度或度量改变后，重新选择失效引用。',
            'Check the chart title, type, legend and required fields. Re-select references after changing declared dimensions or measures.',
          )}
        </DesignWarning>
      )}
      <section
        aria-label={text('图表结构预览', 'Chart structure preview')}
        className="mt-4 rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-4"
      >
        <div className="mb-4 flex flex-wrap items-center justify-between gap-2">
          <h3 className="text-sm font-medium text-[var(--color-text-primary)]">
            {config.title || text('未设置图表标题', 'Chart title not configured')}
          </h3>
          <span className="rounded-md bg-[var(--color-accent-subtle)] px-2 py-1 text-xs text-[var(--color-accent)]">
            {text('结构预览', 'Structure preview')} · {typeLabel(config.type)}
          </span>
        </div>
        {config.type === 'table' ? (
          <div className="overflow-x-auto rounded-md border border-[var(--color-border-default)]">
            <table className="data-table">
              <thead>
                <tr>
                  {previewHeaders.length ? (
                    previewHeaders.map((header) => (
                      <th key={header} scope="col">
                        {header}
                      </th>
                    ))
                  ) : (
                    <th scope="col">{text('待配置返回字段', 'Configure result fields')}</th>
                  )}
                </tr>
              </thead>
              <tbody>
                <tr>
                  <td colSpan={Math.max(1, previewHeaders.length)} className="py-8 text-center">
                    <Table2 size={22} className="mx-auto mb-2 text-[var(--color-text-tertiary)]" />
                    {text('执行真实查询后显示结果行', 'Result rows appear after a real query')}
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
        ) : config.type === 'pie' ? (
          <div className="flex min-h-44 flex-col items-center justify-center gap-3">
            <Circle size={90} strokeWidth={1} className="text-[var(--color-border-hover)]" />
            <p className="text-xs text-[var(--color-text-tertiary)]">
              {text('等待真实分类与度量结果', 'Waiting for actual category and measure results')}
            </p>
          </div>
        ) : (
          <div className="relative ml-9 mt-7 min-h-44 border-b border-l border-[var(--color-border-hover)]">
            <span className="absolute -left-9 -top-6 max-w-40 truncate text-xs text-[var(--color-text-tertiary)]">
              {config.y_field || text('度量', 'Measure')}
            </span>
            <div className="flex min-h-40 flex-col items-center justify-center gap-2">
              <BarChart3 size={24} className="text-[var(--color-text-tertiary)]" />
              <p className="text-xs text-[var(--color-text-tertiary)]">
                {text('执行后绘制真实结果', 'Actual results are drawn after execution')}
              </p>
            </div>
            <span className="absolute -bottom-6 right-0 max-w-40 truncate text-xs text-[var(--color-text-tertiary)]">
              {config.x_field || text('分类', 'Category')}
            </span>
          </div>
        )}
        {config.legend && (
          <p className="mt-8 text-xs text-[var(--color-text-tertiary)]">
            {text('图例字段', 'Legend field')}: {config.y_field || text('待配置', 'Not configured')}
          </p>
        )}
        <p className="mt-5 text-xs text-[var(--color-text-tertiary)]">
          {text(
            '此预览不运行 SQL，不生成随机统计或示例趋势。',
            'This preview does not run SQL or generate sample statistics or trends.',
          )}
        </p>
      </section>
    </fieldset>
  );
}
