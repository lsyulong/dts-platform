import { useState } from 'react';
import { Boxes, LayoutGrid, List } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import ResourceTable from '../ResourceTable';
import { localized, productText } from '../ui-text';
import type { ProductWorkspaceProps } from '../workspace-types';
import { FieldValue, WorkspaceActions, WorkspaceScope, WorkspaceStatus } from './workspace-ui';
import { workspaceText } from './workspace-text';
import { readableValue } from './workspace-utils';
import AssetSourceStatus from '../AssetSourceStatus';

export default function AssetCatalogWorkspace(props: ProductWorkspaceProps) {
  const { i18n } = useTranslation();
  const language = i18n.language;
  const text = productText(language);
  const words = workspaceText(language);
  const [mode, setMode] = useState<'cards' | 'table'>('cards');
  const fields = props.schema.fields
    .filter((field) => !['json', 'textarea'].includes(field.type))
    .slice(0, 4);

  return (
    <div>
      <div className="flex flex-wrap items-start justify-between gap-3 border-b border-[var(--color-border-default)] p-4 sm:px-5">
        <div>
          <h2 className="flex items-center gap-2 text-sm font-semibold">
            <Boxes size={16} className="text-[var(--color-accent)]" />
            {localized(props.module.title, language)} · {words.currentPage}
          </h2>
          <div className="mt-2">
            <WorkspaceScope language={language} />
          </div>
        </div>
        <div
          role="group"
          aria-label={words.displayMode}
          className="flex shrink-0 rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-secondary)] p-1"
        >
          <button
            type="button"
            aria-pressed={mode === 'cards'}
            onClick={() => setMode('cards')}
            className={
              'flex items-center gap-1.5 rounded px-3 py-1.5 text-xs font-medium transition-colors ' +
              (mode === 'cards'
                ? 'bg-[var(--color-bg-elevated)] text-[var(--color-accent)] shadow-sm'
                : 'text-[var(--color-text-secondary)]')
            }
          >
            <LayoutGrid size={14} />
            {words.cards}
          </button>
          <button
            type="button"
            aria-pressed={mode === 'table'}
            onClick={() => setMode('table')}
            className={
              'flex items-center gap-1.5 rounded px-3 py-1.5 text-xs font-medium transition-colors ' +
              (mode === 'table'
                ? 'bg-[var(--color-bg-elevated)] text-[var(--color-accent)] shadow-sm'
                : 'text-[var(--color-text-secondary)]')
            }
          >
            <List size={14} />
            {words.table}
          </button>
        </div>
      </div>
      {mode === 'table' ? (
        <ResourceTable {...props} />
      ) : (
        <div className="grid gap-4 p-4 sm:grid-cols-2 sm:p-5 xl:grid-cols-3">
          {props.records.map((record) => {
            const tags = Array.isArray(record.properties.tags)
              ? record.properties.tags.filter((tag): tag is string => typeof tag === 'string')
              : [];
            return (
              <article
                key={record.id}
                className="flex min-w-0 flex-col rounded-lg border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-4 transition-colors hover:border-[var(--color-border-hover)]"
              >
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <button
                      type="button"
                      className="block max-w-full break-words text-left font-semibold text-[var(--color-accent)] hover:underline"
                      onClick={() => props.onView(record)}
                    >
                      {record.name}
                    </button>
                    <p className="mt-1 break-all text-xs text-[var(--color-text-tertiary)]">
                      {record.code}
                    </p>
                  </div>
                  <WorkspaceStatus status={record.status} />
                </div>
                <p className="mt-3 line-clamp-3 whitespace-pre-wrap break-words text-sm leading-relaxed text-[var(--color-text-secondary)]">
                  {record.description || words.noDescription}
                </p>
                <AssetSourceStatus record={record} />
                <dl className="mt-4 grid flex-1 gap-3 sm:grid-cols-2">
                  {fields.map((field) => (
                    <div key={field.key} className="min-w-0">
                      <dt className="text-xs text-[var(--color-text-tertiary)]">
                        {localized(field.label, language)}
                      </dt>
                      <dd className="mt-1 line-clamp-2 break-words text-sm">
                        <FieldValue field={field} record={record} language={language} />
                      </dd>
                    </div>
                  ))}
                </dl>
                {tags.length > 0 && (
                  <div className="mt-4 flex flex-wrap gap-1.5">
                    {tags.slice(0, 8).map((tag, index) => (
                      <span
                        key={index + '-' + tag}
                        className="rounded bg-[var(--color-accent-subtle)] px-2 py-1 text-xs text-[var(--color-accent)]"
                      >
                        {tag}
                      </span>
                    ))}
                    {tags.length > 8 && (
                      <span className="px-1 py-1 text-xs text-[var(--color-text-tertiary)]">
                        +{tags.length - 8}
                      </span>
                    )}
                  </div>
                )}
                {record.properties.tags !== undefined && !Array.isArray(record.properties.tags) && (
                  <p className="mt-3 break-words text-xs text-[var(--color-text-secondary)]">
                    {readableValue(record.properties.tags, language)}
                  </p>
                )}
                <div className="mt-4 flex flex-wrap items-center justify-between gap-2 border-t border-[var(--color-border-default)] pt-3">
                  <span className="text-xs text-[var(--color-text-tertiary)]">
                    {text.modified}: {new Date(record.updated_at).toLocaleDateString(language)}
                  </span>
                  <WorkspaceActions {...props} record={record} />
                </div>
              </article>
            );
          })}
        </div>
      )}
    </div>
  );
}
