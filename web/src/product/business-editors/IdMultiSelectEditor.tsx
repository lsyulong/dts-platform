import { useDeferredValue, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { X } from 'lucide-react';
import { Button } from '../../components/ui/button';
import { useAuthStore } from '../../stores/auth';
import { duplicateIndexes, stringList } from './business-models';
import { businessText } from './business-text';
import {
  BusinessEditorFrame,
  BusinessWarning,
  inputClass,
  InvalidBusinessValue,
} from './business-ui';
import {
  fetchSelectionOptions,
  selectionSessionScope,
  type SelectionSource,
} from './option-sources';
import type { BusinessPropertyEditorProps } from './types';

const sourceFor = (fieldKey: string): SelectionSource =>
  ['authorized_role_ids', 'role_ids'].includes(fieldKey)
    ? 'roles'
    : fieldKey === 'organization_ids'
      ? 'organization'
      : fieldKey === 'menu_ids'
        ? 'system_menu'
        : fieldKey === 'rule_ids'
          ? 'quality_rule'
          : fieldKey === 'asset_ids'
            ? 'asset_entry'
            : 'users';

export default function IdMultiSelectEditor({
  kind,
  fieldKey,
  value,
  onChange,
  disabled,
}: BusinessPropertyEditorProps) {
  const { i18n } = useTranslation();
  const language = i18n.language;
  const text = businessText(language);
  const user = useAuthStore((state) => state.user);
  const token = useAuthStore((state) => state.token);
  const session = selectionSessionScope(token);
  const [search, setSearch] = useState('');
  const [offset, setOffset] = useState(0);
  const [metricSource, setMetricSource] = useState<SelectionSource>('atomic_metric');
  const source = fieldKey === 'metric_ids' ? metricSource : sourceFor(fieldKey);
  const q = useDeferredValue(search.trim());
  const selected = stringList(value);
  const query = useQuery({
    queryKey: [
      'product-editor-options',
      user?.tenant_id,
      user?.id,
      session,
      source,
      kind,
      q,
      offset,
      language,
    ],
    queryFn: ({ signal }) => {
      if (!user || !token) throw new Error(text.unavailable);
      return fetchSelectionOptions({
        source,
        resourceKind: kind,
        tenantId: user.tenant_id,
        userId: user.id,
        token,
        q,
        offset,
        language,
        signal,
      });
    },
    enabled: !!user && !!token && selected !== null,
    retry: false,
  });
  const options = query.data?.data ?? [];
  const titles: Record<SelectionSource, string> = {
    users: text.users,
    roles: text.roles,
    organization: language.startsWith('zh') ? '组织' : 'Organizations',
    system_menu: text.menu,
    quality_rule: text.rules,
    asset_entry: text.assets,
    atomic_metric: text.atomic,
    derived_metric: text.derived,
    composite_metric: text.composite,
  };
  if (!selected)
    return (
      <InvalidBusinessValue
        text={text.incompatible}
        resetLabel={text.reset}
        onReset={() => onChange([])}
        disabled={disabled}
      />
    );
  const selectionError =
    duplicateIndexes(selected).size > 0
      ? text.duplicate
      : selected.some((id) => !id.trim())
        ? text.missing
        : '';

  return (
    <BusinessEditorFrame title={titles[source]}>
      {selectionError && (
        <>
          <BusinessWarning>{selectionError}</BusinessWarning>
          <input
            className="sr-only"
            tabIndex={-1}
            required
            aria-label={selectionError}
            value=""
            onChange={() => {}}
            disabled={disabled}
          />
        </>
      )}
      {fieldKey === 'metric_ids' && (
        <div className="flex flex-wrap gap-1" role="group" aria-label={text.dataType}>
          {(['atomic_metric', 'derived_metric', 'composite_metric'] as const).map((kind) => (
            <button
              type="button"
              key={kind}
              aria-pressed={source === kind}
              disabled={disabled}
              onClick={() => {
                setMetricSource(kind);
                setOffset(0);
              }}
              className={
                'rounded-md px-3 py-1.5 text-xs font-medium ' +
                (source === kind
                  ? 'bg-[var(--color-accent-subtle)] text-[var(--color-accent)]'
                  : 'text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-tertiary)]')
              }
            >
              {titles[kind]}
            </button>
          ))}
        </div>
      )}
      <input
        type="search"
        className={inputClass}
        aria-label={text.selectSearch}
        placeholder={text.selectSearch}
        value={search}
        disabled={disabled}
        onChange={(event) => {
          setSearch(event.target.value);
          setOffset(0);
        }}
      />
      <p className="text-xs leading-relaxed text-[var(--color-text-tertiary)]">
        {text.optionScope}
      </p>
      {selected.length > 0 && (
        <div className="space-y-2">
          <span className="text-xs font-medium text-[var(--color-text-secondary)]">
            {text.selected}: {selected.length}
          </span>
          <div className="flex flex-wrap gap-1.5">
            {selected.map((id, index) => (
              <span
                key={index + '-' + id}
                className="flex max-w-full items-center gap-1 rounded bg-[var(--color-accent-subtle)] py-1 pl-2 pr-1 text-xs text-[var(--color-accent)]"
              >
                <span className="break-all">
                  {options.find((option) => option.id === id)?.label ?? `${id} · ${text.notLoaded}`}
                </span>
                <button
                  type="button"
                  className="shrink-0 rounded p-1 hover:bg-[var(--color-bg-elevated)] disabled:opacity-40"
                  aria-label={text.remove + ' ' + id}
                  disabled={disabled}
                  onClick={() => onChange(selected.filter((_, current) => current !== index))}
                >
                  <X size={12} />
                </button>
              </span>
            ))}
          </div>
        </div>
      )}
      {!user || !token ? (
        <p className="py-4 text-center text-xs text-[var(--color-text-tertiary)]">
          {text.unavailable}
        </p>
      ) : query.isPending ? (
        <p className="py-4 text-center text-xs text-[var(--color-text-tertiary)]">{text.loading}</p>
      ) : query.error ? (
        <div className="space-y-2">
          <BusinessWarning>{query.error.message}</BusinessWarning>
          <Button
            type="button"
            size="sm"
            variant="outline"
            disabled={disabled || query.isFetching}
            onClick={() => void query.refetch()}
          >
            {text.refresh}
          </Button>
        </div>
      ) : (
        <div className="max-h-60 space-y-1 overflow-y-auto rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-2">
          {options.map((option) => (
            <label
              key={option.id}
              className="flex cursor-pointer items-start gap-2 rounded px-2 py-2 hover:bg-[var(--color-bg-secondary)]"
            >
              <input
                type="checkbox"
                className="mt-0.5 accent-[var(--color-accent)]"
                disabled={disabled}
                checked={selected.includes(option.id)}
                onChange={(event) =>
                  onChange(
                    event.target.checked
                      ? [...selected, option.id]
                      : selected.filter((id) => id !== option.id),
                  )
                }
              />
              <span className="min-w-0">
                <span className="block break-words text-sm">{option.label}</span>
                {option.detail && (
                  <span className="mt-0.5 block break-words text-xs text-[var(--color-text-tertiary)]">
                    {option.detail}
                  </span>
                )}
              </span>
            </label>
          ))}
          {!options.length && (
            <p className="py-4 text-center text-xs text-[var(--color-text-tertiary)]">
              {text.noOptions}
            </p>
          )}
        </div>
      )}
      {query.data && (
        <div className="flex flex-wrap items-center justify-between gap-2 text-xs">
          <span className="text-[var(--color-text-tertiary)]">
            {offset + (options.length ? 1 : 0)}–{offset + options.length} / {query.data.total}
          </span>
          <div className="flex gap-2">
            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled={disabled || !offset || query.isFetching}
              onClick={() => setOffset(Math.max(0, offset - 50))}
            >
              {text.previous}
            </Button>
            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled={disabled || offset + 50 >= query.data.total || query.isFetching}
              onClick={() => setOffset(offset + 50)}
            >
              {text.next}
            </Button>
          </div>
        </div>
      )}
    </BusinessEditorFrame>
  );
}
