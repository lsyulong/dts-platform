import { useTranslation } from 'react-i18next';
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '../components/ui/dialog';
import { ErrorBanner } from '../components/ui';
import { apiErrorMessage } from '../lib/apiError';
import { useResourceAudit } from './api';
import type { ProductModule } from './catalog-types';
import type { ProductResource, ResourceSchema } from './resource-types';
import { localized, productText, statusText } from './ui-text';
import ResourceFieldInput from './ResourceFieldInput';
import AssetSourceStatus from './AssetSourceStatus';

import { formatProperty } from './format-property';

export default function ResourceDetails({
  module,
  schema,
  record,
  onClose,
}: {
  module: ProductModule;
  schema: ResourceSchema;
  record: ProductResource;
  onClose: () => void;
}) {
  const { i18n } = useTranslation();
  const text = productText(i18n.language);
  const audit = useResourceAudit(schema.kind, schema.write_permission ? record.id : undefined);

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <DialogContent className="max-w-4xl">
        <DialogTitle>{record.name}</DialogTitle>
        <DialogDescription>
          {localized(module.title, i18n.language)} · {record.code}
        </DialogDescription>
        <dl className="mt-5 grid gap-4 rounded-md bg-[var(--color-bg-secondary)] p-4 sm:grid-cols-3">
          {[
            [text.status, statusText(record.status, i18n.language)],
            [text.version, String(record.version)],
            [text.modified, new Date(record.updated_at).toLocaleString(i18n.language)],
          ].map(([label, value]) => (
            <div key={label}>
              <dt className="text-xs text-[var(--color-text-tertiary)]">{label}</dt>
              <dd className="mt-1 text-sm font-medium">{value}</dd>
            </div>
          ))}
        </dl>
        <AssetSourceStatus record={record} details />
        {record.description && (
          <p className="mt-4 whitespace-pre-wrap text-sm text-[var(--color-text-secondary)]">
            {record.description}
          </p>
        )}
        <h3 className="mb-3 mt-6 font-medium">{text.definition}</h3>
        <dl className="grid gap-3 sm:grid-cols-2">
          {schema.fields.map((field) => (
            <div
              key={field.key}
              className={field.type === 'json' || field.type === 'textarea' ? 'sm:col-span-2' : ''}
            >
              <dt className="text-xs text-[var(--color-text-tertiary)]">
                {localized(field.label, i18n.language)}
              </dt>
              <dd className="mt-1 whitespace-pre-wrap break-words rounded-md border border-[var(--color-border-default)] p-3 text-sm">
                {field.type === 'json' && record.properties[field.key] !== undefined ? (
                  <ResourceFieldInput
                    kind={schema.kind}
                    field={field}
                    value={record.properties[field.key]}
                    onChange={() => {}}
                    disabled
                    properties={record.properties}
                  />
                ) : (
                  formatProperty(record.properties[field.key], i18n.language)
                )}
              </dd>
            </div>
          ))}
        </dl>
        {schema.write_permission && <h3 className="mb-3 mt-6 font-medium">{text.audit}</h3>}
        {schema.write_permission &&
          (audit.isPending ? (
            <p className="text-sm">{text.loading}</p>
          ) : audit.error ? (
            <ErrorBanner message={apiErrorMessage(audit.error, text.error)} />
          ) : (
            <div className="overflow-x-auto">
              <table className="data-table">
                <thead>
                  <tr>
                    <th>{text.timestamp}</th>
                    <th>{text.operation}</th>
                    <th>{text.actor}</th>
                    <th>{text.changes}</th>
                  </tr>
                </thead>
                <tbody>
                  {audit.data?.map((event) => (
                    <tr key={event.id}>
                      <td className="whitespace-nowrap">
                        {new Date(event.created_at).toLocaleString(i18n.language)}
                      </td>
                      <td>{event.operation}</td>
                      <td>{event.actor_id}</td>
                      <td className="break-words">{JSON.stringify(event.summary)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
              {audit.data?.length === 0 && (
                <p className="py-5 text-center text-sm text-[var(--color-text-tertiary)]">
                  {text.noAudit}
                </p>
              )}
            </div>
          ))}
      </DialogContent>
    </Dialog>
  );
}
