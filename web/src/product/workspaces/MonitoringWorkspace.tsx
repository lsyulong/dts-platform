import { useMemo } from 'react';
import { AlertCircle, BarChart3, CheckSquare, ListChecks } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Button } from '../../components/ui/button';
import ResourceTable from '../ResourceTable';
import type { ProductResource } from '../resource-types';
import { localized, productText, statusText } from '../ui-text';
import type { ProductWorkspaceProps } from '../workspace-types';
import { WorkspaceScope, WorkspaceStatus } from './workspace-ui';
import { workspaceText } from './workspace-text';
import { readableValue } from './workspace-utils';

const resultKeys = new Set([
  'result',
  'results',
  'result_rows',
  'summary',
  'metrics',
  'outcome',
  'evidence',
  'evidence_summary',
  'checked_rows',
  'failed_rows',
  'rows_returned',
  'changes',
]);
const errorKeys = ['error', 'error_message', 'failure_reason', 'last_error', 'failure_message'];
const numericResultKeys = new Set([
  'checked_rows',
  'failed_rows',
  'rows_returned',
  'duration_ms',
  'rows_synced',
  'bytes_synced',
]);

const hasValue = (value: unknown) => value !== null && value !== undefined && value !== '';

function errorFor(record: ProductResource) {
  for (const key of errorKeys) {
    const value = record.properties[key];
    if (typeof value === 'string' && value.trim()) return value;
    if (value && typeof value === 'object' && Object.keys(value).length > 0) return value;
  }
  return undefined;
}

export default function MonitoringWorkspace(props: ProductWorkspaceProps) {
  const { i18n } = useTranslation();
  const language = i18n.language;
  const text = productText(language);
  const words = workspaceText(language);
  const resultFields = props.schema.fields.filter((field) => resultKeys.has(field.key));
  const errors = props.records.filter((record) => errorFor(record) !== undefined);
  const results = props.records.filter((record) =>
    resultFields.some((field) => hasValue(record.properties[field.key])),
  );
  const distribution = useMemo(() => {
    const counts = new Map<string, number>();
    props.records.forEach((record) =>
      counts.set(record.status, (counts.get(record.status) ?? 0) + 1),
    );
    return [...counts.entries()];
  }, [props.records]);
  const numericTotals = props.schema.fields
    .filter((field) => numericResultKeys.has(field.key))
    .flatMap((field) => {
      const values = props.records
        .map((record) => record.properties[field.key])
        .filter((value): value is number => typeof value === 'number' && Number.isFinite(value));
      return values.length
        ? [{ field, value: values.reduce((sum, value) => sum + value, 0), count: values.length }]
        : [];
    });

  return (
    <div>
      <div className="space-y-4 p-4 sm:p-5">
        <div>
          <h2 className="flex items-center gap-2 text-sm font-semibold">
            <BarChart3 size={16} className="text-[var(--color-accent)]" />
            {localized(props.module.title, language)} · {words.currentPage}
          </h2>
          <p className="mt-2 text-xs text-[var(--color-text-tertiary)]">{words.resultScope}</p>
          <div className="mt-1">
            <WorkspaceScope language={language} />
          </div>
        </div>
        <div className="grid gap-3 sm:grid-cols-3">
          {[
            { label: words.recordsOnPage, value: props.records.length, icon: ListChecks },
            { label: words.resultRecords, value: results.length, icon: CheckSquare },
            { label: words.errorRecords, value: errors.length, icon: AlertCircle },
          ].map((metric) => (
            <div
              key={metric.label}
              className="rounded-lg border border-[var(--color-border-default)] bg-[var(--color-bg-secondary)] p-4"
            >
              <div className="flex items-center justify-between gap-2">
                <span className="text-xs text-[var(--color-text-tertiary)]">{metric.label}</span>
                <metric.icon size={16} className="text-[var(--color-accent)]" />
              </div>
              <p className="mt-3 text-2xl font-semibold tabular-nums">
                {metric.value.toLocaleString(language)}
              </p>
            </div>
          ))}
        </div>
        {numericTotals.length > 0 && (
          <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
            {numericTotals.map(({ field, value, count }) => (
              <dl
                key={field.key}
                className="rounded-lg border border-[var(--color-border-default)] p-4"
              >
                <dt className="text-xs text-[var(--color-text-tertiary)]">
                  {localized(field.label, language)} · {words.sumOnPage}
                </dt>
                <dd className="mt-2 text-lg font-semibold tabular-nums">
                  {value.toLocaleString(language)}
                </dd>
                <dd className="mt-1 text-xs text-[var(--color-text-tertiary)]">
                  {count} {text.total}
                </dd>
              </dl>
            ))}
          </div>
        )}
        <div className="grid gap-4 lg:grid-cols-2">
          <section className="rounded-lg border border-[var(--color-border-default)] p-4">
            <h3 className="text-sm font-medium">
              {words.recordedStatuses} · {words.currentPage}
            </h3>
            <div className="mt-4 space-y-4">
              {distribution.map(([status, count]) => (
                <div key={status}>
                  <div className="mb-2 flex items-center justify-between gap-3 text-xs">
                    <span>{statusText(status, language)}</span>
                    <span className="tabular-nums text-[var(--color-text-secondary)]">
                      {count} / {props.records.length}
                    </span>
                  </div>
                  <div
                    className="h-2 overflow-hidden rounded-full bg-[var(--color-bg-tertiary)]"
                    role="meter"
                    aria-label={statusText(status, language)}
                    aria-valuemin={0}
                    aria-valuemax={props.records.length}
                    aria-valuenow={count}
                  >
                    <div
                      className={
                        'h-full rounded-full ' +
                        (['failed', 'rejected', 'unhealthy'].includes(status)
                          ? 'bg-[var(--color-error)]'
                          : 'bg-[var(--color-accent)]')
                      }
                      style={{
                        width: props.records.length
                          ? (count / props.records.length) * 100 + '%'
                          : '0%',
                      }}
                    />
                  </div>
                </div>
              ))}
              {!distribution.length && (
                <p className="py-6 text-center text-sm text-[var(--color-text-tertiary)]">
                  {words.noStatusRecords}
                </p>
              )}
            </div>
          </section>
          <section className="rounded-lg border border-[var(--color-border-default)] p-4">
            <h3 className="text-sm font-medium">
              {words.executionErrors} · {words.currentPage}
            </h3>
            <div className="mt-4 space-y-3">
              {errors.map((record) => (
                <div key={record.id} className="rounded-md bg-[var(--color-error-subtle)] p-3">
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <span className="break-words text-sm font-medium">{record.name}</span>
                    <Button
                      type="button"
                      size="sm"
                      variant="ghost"
                      onClick={() => props.onView(record)}
                    >
                      {text.view}
                    </Button>
                  </div>
                  <p className="mt-1 line-clamp-3 whitespace-pre-wrap break-words text-xs text-[var(--color-text-secondary)]">
                    {readableValue(errorFor(record), language)}
                  </p>
                </div>
              ))}
              {!errors.length && (
                <p className="py-6 text-center text-sm text-[var(--color-text-tertiary)]">
                  {words.noErrors}
                </p>
              )}
            </div>
          </section>
        </div>
        {results.length > 0 && (
          <section className="rounded-lg border border-[var(--color-border-default)] p-4">
            <h3 className="text-sm font-medium">
              {words.resultContent} · {words.currentPage}
            </h3>
            <div className="mt-4 grid gap-3 lg:grid-cols-2">
              {results.map((record) => (
                <article
                  key={record.id}
                  className="min-w-0 rounded-md bg-[var(--color-bg-secondary)] p-3"
                >
                  <div className="flex flex-wrap items-start justify-between gap-2">
                    <button
                      type="button"
                      className="break-words text-left text-sm font-medium text-[var(--color-accent)] hover:underline"
                      onClick={() => props.onView(record)}
                    >
                      {record.name}
                    </button>
                    <WorkspaceStatus status={record.status} />
                  </div>
                  <dl className="mt-3 space-y-2">
                    {resultFields
                      .filter((field) => hasValue(record.properties[field.key]))
                      .slice(0, 3)
                      .map((field) => (
                        <div key={field.key}>
                          <dt className="text-xs text-[var(--color-text-tertiary)]">
                            {localized(field.label, language)}
                          </dt>
                          <dd className="mt-1 line-clamp-3 whitespace-pre-wrap break-words text-xs">
                            {readableValue(record.properties[field.key], language)}
                          </dd>
                        </div>
                      ))}
                  </dl>
                </article>
              ))}
            </div>
          </section>
        )}
      </div>
      <div className="border-t border-[var(--color-border-default)]">
        <ResourceTable {...props} />
      </div>
    </div>
  );
}
