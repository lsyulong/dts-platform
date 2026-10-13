import { useEffect } from 'react';
import ActionParameters from './ActionParameters';
import { useProductResource } from './api';
import { ErrorBanner } from '../components/ui';
import { apiErrorMessage } from '../lib/apiError';
import type { ProductResource } from './resource-types';

type Props = {
  record: ProductResource;
  value: Record<string, unknown>;
  onChange: (value: Record<string, unknown>) => void;
  disabled: boolean;
  text: (zh: string, en: string) => string;
  availabilityKey?: string;
  onAvailability?: (key: string, ready: boolean) => void;
};
export default function RuntimeActionParameters(props: Props) {
  const id =
    typeof props.record.properties.configuration_id === 'string'
      ? props.record.properties.configuration_id
      : undefined;
  const configuration = useProductResource('development_config', id);
  const actual = configuration.data;
  const variables = actual?.properties.variables;
  const usable =
    !!actual &&
    actual.id === id &&
    actual.tenant_id === props.record.tenant_id &&
    actual.status === 'published' &&
    !!variables &&
    typeof variables === 'object' &&
    !Array.isArray(variables);
  const ready = !id || (usable && !configuration.error && !configuration.isFetching);
  const { availabilityKey, onAvailability } = props;
  useEffect(() => {
    if (availabilityKey) onAvailability?.(availabilityKey, ready);
  }, [availabilityKey, onAvailability, ready]);
  return (
    <div className="space-y-3">
      {id && (
        <p className="text-sm text-[var(--color-text-secondary)]">
          {props.text(
            '参数优先级：定义默认值 → 已发布配置 → 本次覆盖。未勾选覆盖时沿用配置或默认值。',
            'Parameter precedence: definition defaults → published configuration → run overrides. Leave overrides unchecked to inherit.',
          )}
        </p>
      )}
      {id && configuration.isPending && (
        <p role="status">{props.text('正在读取运行配置…', 'Loading runtime configuration…')}</p>
      )}
      {configuration.error && (
        <ErrorBanner
          message={apiErrorMessage(
            configuration.error,
            props.text('运行配置读取失败', 'Cannot read runtime configuration'),
          )}
        />
      )}
      {id && !configuration.isPending && !configuration.error && !usable && (
        <ErrorBanner
          message={props.text(
            '配置尚未发布或已经失效，请先处理配置并重新发布开发定义。',
            'The configuration is unpublished or invalid. Publish the configuration and development definition first.',
          )}
        />
      )}
      <ActionParameters
        {...props}
        disabled={props.disabled || (!!id && !usable)}
        inherited={usable ? (variables as Record<string, unknown>) : {}}
      />
    </div>
  );
}
