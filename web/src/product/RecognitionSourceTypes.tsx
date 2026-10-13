import { useTranslation } from 'react-i18next';

export default function RecognitionSourceTypes({
  label,
  value,
  onChange,
  disabled,
}: {
  label: string;
  value: unknown;
  onChange: (value: unknown) => void;
  disabled?: boolean;
}) {
  const { i18n } = useTranslation();
  const zh = i18n.language.startsWith('zh');
  const selected = Array.isArray(value)
    ? value.filter((item): item is string => typeof item === 'string')
    : ['text'];
  const options = [
    ['text', '文本', 'Text'],
    ['number', '数值', 'Number'],
    ['date', '日期时间', 'Date/time'],
    ['boolean', '布尔', 'Boolean'],
  ];
  return (
    <fieldset className="space-y-2">
      <legend className="text-sm font-medium text-[var(--color-text-secondary)]">{label}</legend>
      <div className="flex flex-wrap gap-5">
        {options.map(([key, cn, en]) => (
          <label key={key} className="flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              disabled={disabled}
              checked={selected.includes(key)}
              onChange={(event) =>
                onChange(
                  event.target.checked
                    ? [...selected, key]
                    : selected.filter((item) => item !== key),
                )
              }
            />
            {zh ? cn : en}
          </label>
        ))}
      </div>
      {!selected.length && (
        <p role="alert" className="text-xs text-[var(--color-error)]">
          {zh ? '至少选择一种来源类型' : 'Select at least one source type'}
        </p>
      )}
      <p className="text-xs text-[var(--color-text-tertiary)]">
        {zh
          ? '扫描值时仅检查选定的实际字段类型；字段名识别使用实际结构。'
          : 'Value scans inspect selected actual column types; name scans use the actual schema.'}
      </p>
    </fieldset>
  );
}
