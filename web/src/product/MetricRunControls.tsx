import { useEffect, useId, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { FormInput } from '../components/ui';
import type { ProductResource } from './resource-types';

type Mode = 'explicit' | 'calendar';
const object = (value: unknown): Record<string, unknown> | null =>
  value !== null && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
function frozenMode(record: ProductResource): Mode | null {
  if (record.kind === 'atomic_metric') return 'explicit';
  if (record.kind === 'derived_metric') return 'calendar';
  const publication = object(record.properties.publication);
  const plan = object(publication?.plan);
  if (record.status !== 'published' || !Array.isArray(plan?.measures) || !plan.measures.length)
    return null;
  if (plan.measures.some((measure) => object(measure)?.period != null)) return 'calendar';
  return plan.measures.every((measure) => object(measure)?.period === null) ? 'explicit' : null;
}
function inputUtc(value: unknown) {
  if (typeof value !== 'string') return '';
  const date = new Date(value);
  return Number.isFinite(date.valueOf()) ? date.toISOString().slice(0, -1) : '';
}
function outputUtc(value: string) {
  if (!value) return undefined;
  const date = new Date(`${value}Z`);
  return Number.isFinite(date.valueOf()) ? date.toISOString() : undefined;
}
function cleanPayload(payload: Record<string, unknown>, mode: Mode) {
  const next = { ...payload };
  if (mode === 'calendar') {
    delete next.window_start;
    delete next.window_end;
  } else delete next.as_of;
  return next;
}

export default function MetricRunControls({
  record,
  payload,
  onChange,
  disabled = false,
  text: suppliedText,
}: {
  record: ProductResource;
  payload: Record<string, unknown>;
  onChange: (payload: Record<string, unknown>) => void;
  disabled?: boolean;
  text?: (zh: string, en: string) => string;
}) {
  const { i18n } = useTranslation();
  const text =
    suppliedText ?? ((zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en));
  const id = useId();
  const [fallbackMode, setFallbackMode] = useState<Mode>('explicit');
  const inferredMode = frozenMode(record);
  const mode = inferredMode ?? fallbackMode;
  const endInput = useRef<HTMLInputElement>(null);
  const start = typeof payload.window_start === 'string' ? Date.parse(payload.window_start) : NaN;
  const end = typeof payload.window_end === 'string' ? Date.parse(payload.window_end) : NaN;
  const invalidWindow =
    mode === 'explicit' && Number.isFinite(start) && Number.isFinite(end)
      ? start >= end || end - start > 3660 * 86400000
      : false;
  const windowError = text(
    '窗口终点须晚于起点，且跨度不超过3660天',
    'The end must follow the start; the window may not exceed 3660 days',
  );
  useEffect(() => {
    endInput.current?.setCustomValidity(invalidWindow ? windowError : '');
  }, [invalidWindow, windowError]);
  const publication = object(record.properties.publication);
  const plan = object(publication?.plan);
  const timezone = typeof plan?.time_timezone === 'string' ? plan.time_timezone : null;
  const references = Array.isArray(publication?.references) ? publication.references : [];
  const dateInput = (key: 'window_start' | 'window_end' | 'as_of', label: string) => (
    <label className="block text-sm text-[var(--color-text-secondary)]" htmlFor={`${id}-${key}`}>
      <span className="mb-1.5 block font-medium">{label} *</span>
      <input
        ref={key === 'window_end' ? endInput : undefined}
        id={`${id}-${key}`}
        type="datetime-local"
        step="0.001"
        required
        disabled={disabled}
        value={inputUtc(payload[key])}
        aria-invalid={key === 'window_end' && invalidWindow}
        className="w-full rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] px-3 py-2 focus:border-[var(--color-accent)] focus:outline-none"
        onChange={(event) =>
          onChange({ ...cleanPayload(payload, mode), [key]: outputUtc(event.target.value) })
        }
      />
    </label>
  );
  return (
    <section className="space-y-4" aria-label={text('指标计算窗口', 'Metric calculation window')}>
      <p className="rounded-md bg-[var(--color-bg-secondary)] p-3 text-sm">
        {mode === 'calendar'
          ? text(
              '按已发布周期计算完整结束的自然期。下方 as_of 为固定 UTC 时间，重试保持同一口径。',
              'Published periods determine completed calendar windows. The fixed UTC as_of preserves the same window on retry.',
            )
          : text(
              '指定明确的 UTC 起止窗口，计算采用起点包含、终点不包含的区间。',
              'Choose explicit UTC start and end times. The interval includes the start and excludes the end.',
            )}
        {timezone && (
          <span className="mt-1 block">
            {text('已发布业务时区', 'Published business timezone')}: {timezone}
          </span>
        )}
      </p>
      {!inferredMode && (
        <label className="block text-sm">
          <span className="mb-1.5 block">
            {text('窗口模式（服务端核验）', 'Window mode (verified by server)')}
          </span>
          <select
            value={fallbackMode}
            disabled={disabled}
            className="w-full rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-2"
            onChange={(event) => {
              const next = event.target.value === 'calendar' ? 'calendar' : 'explicit';
              setFallbackMode(next);
              onChange(cleanPayload(payload, next));
            }}
          >
            <option value="explicit">{text('明确起止窗口', 'Explicit start and end')}</option>
            <option value="calendar">{text('已发布自然周期', 'Published calendar period')}</option>
          </select>
          <span className="mt-1 block text-xs text-[var(--color-text-tertiary)]">
            {text(
              '当前没有完整冻结周期证明。选择不会改变指标定义，服务端会核验真实依赖及窗口。',
              'A complete frozen period is unavailable. This selection does not change the definition; the server verifies actual dependencies and the window.',
            )}
          </span>
        </label>
      )}
      {mode === 'calendar' ? (
        dateInput('as_of', text('截至时间 as_of（UTC）', 'As of (UTC)'))
      ) : (
        <div className="grid gap-3 sm:grid-cols-2">
          {dateInput(
            'window_start',
            text('窗口起点（UTC，包含）', 'Window start (UTC, inclusive)'),
          )}
          {dateInput('window_end', text('窗口终点（UTC，不包含）', 'Window end (UTC, exclusive)'))}
        </div>
      )}
      {invalidWindow && (
        <p role="alert" className="text-sm text-[var(--color-error)]">
          {windowError}
        </p>
      )}
      <FormInput
        label={text('结果分组上限（1–1000）', 'Result group limit (1–1000)')}
        type="number"
        min={1}
        max={1000}
        step={1}
        required
        disabled={disabled}
        value={typeof payload.row_limit === 'number' ? payload.row_limit : 1000}
        onChange={(value) =>
          onChange({
            ...cleanPayload(payload, mode),
            row_limit: value === '' ? undefined : Number(value),
          })
        }
      />
      {!!references.length && (
        <details className="rounded-md border border-[var(--color-border-default)] p-3 text-sm">
          <summary className="cursor-pointer">
            {text('当前发布的冻结依赖', 'Frozen publication dependencies')}
          </summary>
          <ul className="mt-2 space-y-1">
            {references.flatMap((value, index) => {
              const ref = object(value);
              return ref &&
                typeof ref.code === 'string' &&
                typeof ref.kind === 'string' &&
                typeof ref.version === 'number' ? (
                <li key={`${String(ref.id)}:${index}`}>
                  {ref.code} · {ref.kind} · v{String(ref.version)}
                </li>
              ) : (
                []
              );
            })}
          </ul>
        </details>
      )}
    </section>
  );
}
