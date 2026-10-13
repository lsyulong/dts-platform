import { useMemo, useState } from 'react';
import { ChevronDown, ChevronRight, Folder, GitBranch, Network } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Button } from '../../components/ui/button';
import type { ProductResource } from '../resource-types';
import { localized, productText } from '../ui-text';
import type { ProductWorkspaceProps } from '../workspace-types';
import { buildResourceTree } from './tree-model';
import { FieldValue, WorkspaceActions, WorkspaceScope, WorkspaceStatus } from './workspace-ui';
import { workspaceText } from './workspace-text';

export default function ResourceTreeWorkspace(props: ProductWorkspaceProps) {
  const { i18n } = useTranslation();
  const language = i18n.language;
  const text = productText(language);
  const words = workspaceText(language);
  const tree = useMemo(() => buildResourceTree(props.records), [props.records]);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const selected = props.records.find((record) => record.id === selectedId);
  const toggle = (id: string) =>
    setExpanded((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  const renderNodes = (records: ProductResource[], depth = 0) => (
    <ul className="space-y-1">
      {records.map((record) => {
        const children = tree.children.get(record.id) ?? [];
        const isExpanded = expanded.has(record.id);
        const isSelected = selectedId === record.id;
        return (
          <li key={record.id}>
            <div
              className={
                'group rounded-md border transition-colors ' +
                (isSelected
                  ? 'border-[var(--color-accent)] bg-[var(--color-accent-subtle)]'
                  : 'border-transparent hover:bg-[var(--color-bg-secondary)]')
              }
            >
              <div
                className="flex min-w-0 items-center gap-1 px-2 py-2"
                style={{ paddingLeft: Math.min(depth, 6) * 14 + 8 }}
              >
                {children.length ? (
                  <button
                    type="button"
                    className="shrink-0 rounded p-1.5 hover:bg-[var(--color-bg-tertiary)]"
                    aria-label={(isExpanded ? words.collapse : words.expand) + ' ' + record.name}
                    aria-expanded={isExpanded}
                    onClick={() => toggle(record.id)}
                  >
                    {isExpanded ? <ChevronDown size={15} /> : <ChevronRight size={15} />}
                  </button>
                ) : (
                  <span className="w-7 shrink-0" />
                )}
                {children.length ? (
                  <Folder size={16} className="shrink-0 text-[var(--color-accent)]" />
                ) : (
                  <GitBranch size={15} className="shrink-0 text-[var(--color-text-tertiary)]" />
                )}
                <button
                  type="button"
                  className="min-w-0 flex-1 text-left"
                  aria-label={record.name}
                  aria-pressed={isSelected}
                  onClick={() => setSelectedId(record.id)}
                >
                  <span className="block truncate text-sm font-medium">{record.name}</span>
                  <span className="mt-0.5 block truncate text-xs text-[var(--color-text-tertiary)]">
                    {record.code}
                  </span>
                </button>
                <WorkspaceStatus status={record.status} />
              </div>
              {tree.missingParents.has(record.id) && (
                <p className="px-4 pb-2 text-xs text-[var(--color-warning)]">
                  {words.missingParent}
                </p>
              )}
              {tree.cycles.has(record.id) && (
                <p className="px-4 pb-2 text-xs text-[var(--color-error)]">{words.cycle}</p>
              )}
              <div className="flex justify-end px-2 pb-1">
                <WorkspaceActions {...props} record={record} />
              </div>
            </div>
            {isExpanded && children.length > 0 && (
              <div className="mt-1">{renderNodes(children, depth + 1)}</div>
            )}
          </li>
        );
      })}
    </ul>
  );

  return (
    <div className="p-4 sm:p-5">
      <div className="mb-4 flex flex-wrap items-start justify-between gap-3">
        <div>
          <h2 className="flex items-center gap-2 text-sm font-semibold">
            <Network size={16} className="text-[var(--color-accent)]" />
            {words.tree} · {words.currentPage}
          </h2>
          <div className="mt-2">
            <WorkspaceScope language={language} />
          </div>
        </div>
        <div className="flex flex-wrap gap-2">
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={!tree.children.size}
            onClick={() => setExpanded(new Set(tree.children.keys()))}
          >
            {words.expandAll}
          </Button>
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={!expanded.size}
            onClick={() => setExpanded(new Set())}
          >
            {words.collapseAll}
          </Button>
        </div>
      </div>
      <div className="grid gap-5 lg:grid-cols-[minmax(0,1.2fr)_minmax(0,1fr)]">
        <div
          className="min-w-0 rounded-lg border border-[var(--color-border-default)] p-2"
          aria-label={localized(props.module.title, language)}
        >
          {renderNodes(tree.roots)}
        </div>
        <aside className="min-w-0 rounded-lg border border-[var(--color-border-default)] bg-[var(--color-bg-secondary)] p-4">
          <h3 className="text-sm font-semibold">{words.selectedNode}</h3>
          {selected ? (
            <>
              <div className="mt-4 flex flex-wrap items-start justify-between gap-3">
                <div className="min-w-0">
                  <p className="break-words font-medium">{selected.name}</p>
                  <p className="mt-1 break-all text-xs text-[var(--color-text-tertiary)]">
                    {selected.code}
                  </p>
                </div>
                <WorkspaceStatus status={selected.status} />
              </div>
              {selected.description && (
                <p className="mt-3 whitespace-pre-wrap break-words text-sm text-[var(--color-text-secondary)]">
                  {selected.description}
                </p>
              )}
              <dl className="mt-4 space-y-3">
                <div>
                  <dt className="text-xs text-[var(--color-text-tertiary)]">{text.parent}</dt>
                  <dd className="mt-1 break-all text-sm">
                    {selected.parent_id
                      ? (props.records.find((record) => record.id === selected.parent_id)?.name ??
                        selected.parent_id)
                      : text.none}
                  </dd>
                </div>
                {props.schema.fields.slice(0, 5).map((field) => (
                  <div key={field.key}>
                    <dt className="text-xs text-[var(--color-text-tertiary)]">
                      {localized(field.label, language)}
                    </dt>
                    <dd className="mt-1 line-clamp-3 whitespace-pre-wrap break-words text-sm">
                      <FieldValue field={field} record={selected} language={language} />
                    </dd>
                  </div>
                ))}
              </dl>
              <Button
                type="button"
                variant="outline"
                className="mt-5 w-full"
                onClick={() => props.onView(selected)}
              >
                {text.details}
              </Button>
            </>
          ) : (
            <p className="py-12 text-center text-sm text-[var(--color-text-tertiary)]">
              {words.selectNode}
            </p>
          )}
        </aside>
      </div>
    </div>
  );
}
