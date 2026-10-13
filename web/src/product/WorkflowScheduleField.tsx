import { useTranslation } from 'react-i18next';
import { FormInput } from '../components/ui';
import type { ResourceField } from './catalog-types';
import { localized } from './ui-text';

interface Props {
  field: ResourceField;
  value: unknown;
  onChange: (value: unknown) => void;
  disabled?: boolean;
}

export default function WorkflowScheduleField({ field, value, onChange, disabled }: Props) {
  const { i18n } = useTranslation();
  const zh = i18n.language.startsWith('zh');
  const retry = field.key === 'retry_limit';
  const hint = retry
    ? zh
      ? '0–10 的整数。当前仅支持运行实例的人工重试，尚未启用周期自动重试。'
      : 'Integer from 0 to 10. Run retries are manual; scheduled automatic retries are not enabled.'
    : field.key === 'timezone'
      ? zh
        ? '使用 IANA 名称，如 UTC、Asia/Shanghai。当前社区调度适配只支持 UTC；保存有效时区不代表已注册调度。'
        : 'Use an IANA name such as UTC or Asia/Shanghai. The current community scheduler adapter supports UTC; saving a time zone does not register a schedule.'
      : zh
        ? '五字段：分 时 日 月 星期；六字段在最前面增加秒。星期 0/7 表示周日，支持范围、列表及正整数步长。'
        : 'Five fields: minute hour day month weekday; six fields start with seconds. Weekday 0/7 is Sunday. Ranges, lists and positive steps are supported.';
  return (
    <div className="space-y-1.5">
      <FormInput
        label={localized(field.label, i18n.language)}
        type={retry ? 'number' : 'text'}
        value={typeof value === 'string' || typeof value === 'number' ? value : ''}
        onChange={(next) =>
          retry ? onChange(next === '' ? undefined : Number(next)) : onChange(next)
        }
        required={field.required}
        disabled={disabled}
        min={retry ? 0 : undefined}
        max={retry ? 10 : undefined}
        step={retry ? 1 : undefined}
        placeholder={
          field.key === 'cron_expression'
            ? '0 8 * * MON-FRI'
            : field.key === 'timezone'
              ? 'UTC'
              : undefined
        }
      />
      <p className="text-xs text-[var(--color-text-tertiary)]">{hint}</p>
    </div>
  );
}
