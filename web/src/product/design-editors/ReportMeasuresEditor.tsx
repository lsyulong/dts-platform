import { Trash2 } from 'lucide-react';
import { Button } from '../../components/ui/button';
import {
  aggregates,
  duplicateNames,
  reportMeasures,
  type DesignEditorProps,
  type ReportMeasure,
} from './design-models';
import { DesignHeader, DesignWarning } from './design-ui';
import { designField, designInput, designPanel, useDesignText } from './design-text';

export default function ReportMeasuresEditor({ value, onChange, disabled }: DesignEditorProps) {
  const text = useDesignText();
  const measures = reportMeasures(value);
  const update = (index: number, patch: Partial<ReportMeasure>) =>
    onChange(
      measures.map((measure, position) =>
        position === index ? { ...measure, ...patch } : measure,
      ),
    );
  const invalid =
    measures.some((measure) => !measure.name.trim() || !measure.expression.trim()) ||
    duplicateNames(measures.map((measure) => measure.name)).length > 0;
  return (
    <fieldset className={designPanel} disabled={disabled}>
      <legend className="sr-only">{text('分析度量', 'Analysis measures')}</legend>
      <DesignHeader
        title={text('分析度量', 'Analysis measures')}
        button={text('添加度量', 'Add measure')}
        disabled={disabled}
        onAdd={() => onChange([...measures, { name: '', expression: '', aggregate: 'sum' }])}
      />
      <p className="mb-3 text-xs leading-5 text-[var(--color-text-tertiary)]">
        {text(
          '选择实际采集字段和聚合方式，设置结果别名。COUNT 可使用 * 统计行数。',
          'Choose a collected column and aggregate, then set its result alias. COUNT can use * to count rows.',
        )}
      </p>
      <div className="space-y-3">
        {measures.map((measure, index) => (
          <div
            key={index}
            className="grid items-end gap-3 rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-3 sm:grid-cols-[1fr_1.5fr_100px_auto]"
          >
            <label className={designField}>
              <span>
                {text('度量名称', 'Measure name')} {index + 1}
              </span>
              <input
                className={designInput}
                value={measure.name}
                required
                maxLength={128}
                onChange={(event) => update(index, { name: event.target.value })}
              />
            </label>
            <label className={designField}>
              <span>
                {text('来源字段', 'Source column')} {index + 1}
              </span>
              <input
                className={designInput}
                value={measure.expression}
                required
                maxLength={256}
                onChange={(event) => update(index, { expression: event.target.value })}
              />
            </label>
            <label className={designField}>
              <span>
                {text('聚合方式', 'Aggregation')} {index + 1}
              </span>
              <select
                className={designInput}
                value={measure.aggregate}
                onChange={(event) => {
                  const aggregate = aggregates.find((value) => value === event.target.value);
                  if (aggregate) update(index, { aggregate });
                }}
              >
                {aggregates.map((aggregate) => (
                  <option key={aggregate} value={aggregate}>
                    {aggregate.toUpperCase()}
                  </option>
                ))}
              </select>
            </label>
            <Button
              type="button"
              variant="ghost"
              size="icon"
              aria-label={`${text('删除度量', 'Remove measure')} ${index + 1}`}
              onClick={() => onChange(measures.filter((_, position) => position !== index))}
            >
              <Trash2 />
            </Button>
          </div>
        ))}
      </div>
      {!measures.length && (
        <p className="py-4 text-center text-xs text-[var(--color-text-tertiary)]">
          {text('尚未配置度量', 'No measures configured')}
        </p>
      )}
      {invalid && (
        <DesignWarning>
          {text(
            '度量名称和表达式不能为空，度量名称不能重复。',
            'Measure names and expressions are required; measure names must be unique.',
          )}
        </DesignWarning>
      )}
    </fieldset>
  );
}
