import { Trash2 } from 'lucide-react';
import { Button } from '../../components/ui/button';
import { duplicateNames, reportDimensions, type DesignEditorProps } from './design-models';
import { DesignHeader, DesignWarning } from './design-ui';
import { designField, designInput, designPanel, useDesignText } from './design-text';

export default function ReportDimensionsEditor({ value, onChange, disabled }: DesignEditorProps) {
  const text = useDesignText();
  const dimensions = reportDimensions(value);
  const invalid = dimensions.some((name) => !name.trim()) || duplicateNames(dimensions).length > 0;
  return (
    <fieldset className={designPanel} disabled={disabled}>
      <legend className="sr-only">{text('分析维度', 'Analysis dimensions')}</legend>
      <DesignHeader
        title={text('分析维度', 'Analysis dimensions')}
        button={text('添加维度', 'Add dimension')}
        disabled={disabled}
        onAdd={() => onChange([...dimensions, ''])}
      />
      <p className="mb-3 text-xs text-[var(--color-text-tertiary)]">
        {text(
          '使用实际采集数据对象的字段进行分组，留空则汇总全表。',
          'Group by columns from the collected data object, or leave empty for table totals.',
        )}
      </p>
      <div className="space-y-2">
        {dimensions.map((name, index) => (
          <div key={index} className="flex items-end gap-2">
            <label className={`${designField} flex-1`}>
              <span>
                {text('维度字段', 'Dimension field')} {index + 1}
              </span>
              <input
                className={designInput}
                value={name}
                required
                maxLength={128}
                onChange={(event) =>
                  onChange(
                    dimensions.map((value, position) =>
                      position === index ? event.target.value : value,
                    ),
                  )
                }
              />
            </label>
            <Button
              type="button"
              variant="ghost"
              size="icon"
              aria-label={`${text('删除维度', 'Remove dimension')} ${index + 1}`}
              onClick={() => onChange(dimensions.filter((_, position) => position !== index))}
            >
              <Trash2 />
            </Button>
          </div>
        ))}
      </div>
      {!dimensions.length && (
        <p className="py-4 text-center text-xs text-[var(--color-text-tertiary)]">
          {text('尚未配置维度', 'No dimensions configured')}
        </p>
      )}
      {invalid && (
        <DesignWarning>
          {text('维度名称不能为空，也不能重复。', 'Dimension names must be non-empty and unique.')}
        </DesignWarning>
      )}
    </fieldset>
  );
}
