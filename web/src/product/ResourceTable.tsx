import { useTranslation } from 'react-i18next';
import { FileText, Pencil, Trash2 } from 'lucide-react';
import type { ProductResource, ResourceSchema } from './resource-types';
import { formatProperty } from './format-property';
import { localized, productText, statusText } from './ui-text';
import { actionAllowed, actionPermission, canUseResourceAction } from './action-models';
import { useAuthStore } from '../stores/auth';
import AssetSourceStatus from './AssetSourceStatus';

interface Props {
  schema: ResourceSchema;
  records: ProductResource[];
  canWrite: boolean;
  onView: (record: ProductResource) => void;
  onEdit: (record: ProductResource) => void;
  onDelete: (record: ProductResource) => void;
  onAction: (record: ProductResource, action: string) => void;
  busy?: boolean;
}

export default function ResourceTable({
  schema,
  records,
  canWrite,
  onView,
  onEdit,
  onDelete,
  onAction,
  busy,
}: Props) {
  const { i18n } = useTranslation();
  const user = useAuthStore((state) => state.user);
  const text = productText(i18n.language);
  const columns = schema.fields
    .filter((field) => !['json', 'textarea'].includes(field.type))
    .slice(0, 3);
  const actions = schema.actions.filter(
    (action) => !['create', 'update', 'delete', 'search', 'export'].includes(action),
  );
  return (
    <div className="overflow-x-auto">
      <table className="data-table">
        <thead>
          <tr>
            <th>{text.name}</th>
            <th>{text.code}</th>
            {columns.map((field) => (
              <th key={field.key}>{localized(field.label, i18n.language)}</th>
            ))}
            <th>{text.status}</th>
            <th>{text.modified}</th>
            <th>{text.operations}</th>
          </tr>
        </thead>
        <tbody>
          {records.map((record) => (
            <tr key={record.id}>
              <td>
                <button
                  type="button"
                  className="font-medium text-[var(--color-accent)] hover:underline"
                  onClick={() => onView(record)}
                >
                  {record.name}
                </button>
              </td>
              <td>{record.code}</td>
              {columns.map((field) => (
                <td key={field.key} className="max-w-56 truncate">
                  {formatProperty(record.properties[field.key], i18n.language)}
                </td>
              ))}
              <td>
                <span className="whitespace-nowrap rounded bg-[var(--color-bg-tertiary)] px-2 py-1 text-xs">
                  {statusText(record.status, i18n.language)}
                </span>
                <AssetSourceStatus record={record} />
              </td>
              <td className="whitespace-nowrap text-[var(--color-text-tertiary)]">
                {new Date(record.updated_at).toLocaleString(i18n.language)}
              </td>
              <td>
                <div className="flex items-center gap-1">
                  <button
                    type="button"
                    aria-label={`${text.view} ${record.name}`}
                    title={text.view}
                    className="rounded p-1.5 hover:bg-[var(--color-bg-tertiary)]"
                    onClick={() => onView(record)}
                  >
                    <FileText size={15} />
                  </button>
                  {canWrite && schema.actions.includes('update') && (
                    <button
                      type="button"
                      aria-label={`${text.edit} ${record.name}`}
                      title={text.edit}
                      className="rounded p-1.5 hover:bg-[var(--color-bg-tertiary)]"
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
                        className="whitespace-nowrap rounded px-2 py-1 text-xs text-[var(--color-accent)] hover:bg-[var(--color-accent-subtle)]"
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
                      aria-label={`${text.remove} ${record.name}`}
                      title={text.remove}
                      className="rounded p-1.5 text-[var(--color-error)] hover:bg-[var(--color-error-subtle)]"
                      disabled={busy || !actionAllowed(record, 'delete')}
                      onClick={() => onDelete(record)}
                    >
                      <Trash2 size={15} />
                    </button>
                  )}
                </div>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
