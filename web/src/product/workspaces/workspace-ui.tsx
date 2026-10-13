import { FileText, Pencil, Trash2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import type { ResourceField } from '../catalog-types';
import { formatProperty } from '../format-property';
import type { ProductResource } from '../resource-types';
import { localized, productText, statusText } from '../ui-text';
import type { ProductWorkspaceProps } from '../workspace-types';
import { workspaceText } from './workspace-text';
import { actionAllowed, actionPermission, canUseResourceAction } from '../action-models';
import { useAuthStore } from '../../stores/auth';

export function WorkspaceScope({ language }: { language: string }) {
  return (
    <p className="text-xs leading-relaxed text-[var(--color-text-tertiary)]">
      {workspaceText(language).pageScope}
    </p>
  );
}

export function WorkspaceStatus({ status }: { status: string }) {
  const { i18n } = useTranslation();
  const appearance = [
    'failed',
    'rejected',
    'unhealthy',
    'registration_unknown',
    'stop_unknown',
  ].includes(status)
    ? 'bg-[var(--color-error-subtle)] text-[var(--color-error)]'
    : ['succeeded', 'completed', 'active', 'approved', 'published', 'healthy'].includes(status)
      ? 'bg-[var(--color-success-subtle)] text-[var(--color-success)]'
      : ['pending', 'submitted', 'running', 'registering', 'stopping'].includes(status)
        ? 'bg-[var(--color-accent-subtle)] text-[var(--color-accent)]'
        : 'bg-[var(--color-bg-tertiary)] text-[var(--color-text-secondary)]';
  return (
    <span
      className={
        'inline-flex whitespace-nowrap rounded px-2 py-1 text-xs font-medium ' + appearance
      }
    >
      {statusText(status, i18n.language)}
    </span>
  );
}

export function WorkspaceActions({
  record,
  schema,
  canWrite,
  busy,
  onView,
  onEdit,
  onDelete,
  onAction,
  excludeActions = [],
}: Pick<
  ProductWorkspaceProps,
  'schema' | 'canWrite' | 'busy' | 'onView' | 'onEdit' | 'onDelete' | 'onAction'
> & { record: ProductResource; excludeActions?: readonly string[] }) {
  const { i18n } = useTranslation();
  const text = productText(i18n.language);
  const user = useAuthStore((state) => state.user);
  const actions = schema.actions.filter(
    (action) =>
      !['create', 'update', 'delete', 'search', 'export'].includes(action) &&
      !excludeActions.includes(action),
  );
  const iconClass =
    'rounded p-2 transition-colors hover:bg-[var(--color-bg-tertiary)] disabled:opacity-40';
  return (
    <div className="flex flex-wrap items-center gap-1">
      <button
        type="button"
        title={text.view}
        aria-label={text.view + ' ' + record.name}
        className={iconClass}
        onClick={() => onView(record)}
      >
        <FileText size={15} />
      </button>
      {canWrite && schema.actions.includes('update') && (
        <button
          type="button"
          title={text.edit}
          aria-label={text.edit + ' ' + record.name}
          className={iconClass}
          disabled={busy || !actionAllowed(record, 'update')}
          onClick={() => onEdit(record)}
        >
          <Pencil size={15} />
        </button>
      )}
      {actions
        .filter((action) =>
          canUseResourceAction(
            record.kind,
            action,
            !!user?.permissions.includes(schema.read_permission),
            canWrite,
            user?.permissions ?? [],
          ),
        )
        .map((action) => (
          <button
            type="button"
            key={action}
            className="rounded px-2 py-1.5 text-xs font-medium text-[var(--color-accent)] transition-colors hover:bg-[var(--color-accent-subtle)] disabled:opacity-40"
            disabled={
              busy ||
              !actionAllowed(record, action) ||
              (!!actionPermission(record.kind, action) &&
                !user?.permissions.includes(actionPermission(record.kind, action)!))
            }
            onClick={() => onAction(record, action)}
          >
            {statusText(action, i18n.language)}
          </button>
        ))}
      {canWrite && schema.actions.includes('delete') && (
        <button
          type="button"
          title={text.remove}
          aria-label={text.remove + ' ' + record.name}
          className={iconClass + ' text-[var(--color-error)]'}
          disabled={busy || !actionAllowed(record, 'delete')}
          onClick={() => onDelete(record)}
        >
          <Trash2 size={15} />
        </button>
      )}
    </div>
  );
}

export function FieldValue({
  field,
  record,
  language,
}: {
  field: ResourceField;
  record: ProductResource;
  language: string;
}) {
  const value = record.properties[field.key];
  const option = field.options?.find((item) => item.value === value);
  return <>{option ? localized(option.label, language) : formatProperty(value, language)}</>;
}
