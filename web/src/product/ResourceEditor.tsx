import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '../components/ui/button';
import { ErrorBanner, FormInput } from '../components/ui';
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '../components/ui/dialog';
import { apiErrorMessage } from '../lib/apiError';
import { captureSession, isCurrentSession } from '../lib/session';
import type { ProductModule } from './catalog-types';
import { useResourceMutation } from './api';
import type { ProductResource, ResourceInput, ResourceSchema } from './resource-types';
import ResourceFieldInput from './ResourceFieldInput';
import ReferencePicker from './ReferencePicker';
import { localized, productText } from './ui-text';

interface Props {
  module: ProductModule;
  schema: ResourceSchema;
  existing?: ProductResource;
  onClose: () => void;
}

function makeProperties(
  values: Record<string, unknown>,
  schema: ResourceSchema,
): Record<string, unknown> {
  return Object.fromEntries(
    schema.fields.flatMap((field) => {
      if (
        schema.kind === 'development_config' &&
        values.scope !== 'source' &&
        ['source_kind', 'source_id'].includes(field.key)
      )
        return [];
      if (field.readOnly || (schema.kind === 'realtime_task' && field.key === 'checkpoint'))
        return [];
      const value = values[field.key];
      if (value === '' || value === undefined || value === null) {
        if (field.required) throw new Error(field.label.zh);
        return [];
      }
      if (field.type === 'number') {
        const parsed = Number(value);
        if (!Number.isFinite(parsed)) throw new Error(field.label.zh);
        return [[field.key, parsed]];
      }
      if (field.type === 'json' && typeof value === 'string') {
        return [[field.key, JSON.parse(value)]];
      }
      return [[field.key, value]];
    }),
  );
}

export default function ResourceEditor({ module, schema, existing, onClose }: Props) {
  const { i18n } = useTranslation();
  const text = productText(i18n.language);
  const [code, setCode] = useState(existing?.code ?? '');
  const [name, setName] = useState(existing?.name ?? '');
  const [description, setDescription] = useState(existing?.description ?? '');
  const [parentId, setParentId] = useState(existing?.parent_id ?? '');
  const [values, setValues] = useState<Record<string, unknown>>(() => ({
    ...Object.fromEntries(
      schema.fields.filter((field) => field.type === 'boolean').map((field) => [field.key, false]),
    ),
    ...(schema.kind === 'development_config' ? { scope: 'shared', variables: {} } : {}),
    ...existing?.properties,
  }));
  const [error, setError] = useState('');
  const mutation = useResourceMutation(schema.kind);

  const handleSave = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (mutation.isPending) return;
    if (!event.currentTarget.reportValidity()) return;
    const session = captureSession();
    setError('');
    try {
      const input: ResourceInput = {
        code: code.trim(),
        name: name.trim(),
        description: description.trim() || null,
        parent_id: parentId || null,
        properties: makeProperties(values, schema),
      };
      if (!input.code || !input.name) {
        setError(text.required);
        return;
      }
      await mutation.mutateAsync(
        existing
          ? { type: 'update', id: existing.id, version: existing.version, input }
          : { type: 'create', input },
      );
      if (isCurrentSession(session)) onClose();
    } catch (caught) {
      if (isCurrentSession(session)) setError(apiErrorMessage(caught, text.error));
    }
  };

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !mutation.isPending) onClose();
      }}
    >
      <DialogContent className="max-w-3xl">
        <DialogTitle>
          {existing ? text.edit : text.create} · {localized(module.title, i18n.language)}
        </DialogTitle>
        <DialogDescription>{localized(module.description, i18n.language)}</DialogDescription>
        <form onSubmit={handleSave} className="mt-5 space-y-5">
          {schema.kind === 'master_data_record' && existing?.status === 'approved' && (
            <p className="rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-secondary)] p-3 text-sm">
              {i18n.language.startsWith('zh')
                ? '本次保存将创建新草稿，需要重新提交审批。此前批准的版本会保留。'
                : 'Saving creates a new draft requiring approval. The previous approved version is retained.'}
            </p>
          )}
          <div className="grid gap-4 sm:grid-cols-2">
            <FormInput
              label={text.code}
              value={code}
              onChange={setCode}
              required
              disabled={mutation.isPending}
            />
            <FormInput
              label={text.name}
              value={name}
              onChange={setName}
              required
              disabled={mutation.isPending}
            />
          </div>
          <FormInput
            label={text.description}
            type="textarea"
            value={description}
            onChange={setDescription}
            disabled={mutation.isPending}
          />
          {[
            'organization',
            'system_menu',
            'warehouse_category',
            'asset_directory',
            'development_folder',
          ].includes(schema.kind) && (
            <ReferencePicker
              kind={schema.kind}
              label={text.parent}
              value={parentId}
              onChange={setParentId}
              excludeId={existing?.id}
              disabled={mutation.isPending}
            />
          )}
          <div className="grid gap-4 sm:grid-cols-2">
            {schema.fields
              .filter((field) => !field.readOnly)
              .map((field) => (
                <div
                  key={field.key}
                  className={
                    field.type === 'json' || field.type === 'textarea' ? 'sm:col-span-2' : ''
                  }
                >
                  <ResourceFieldInput
                    kind={schema.kind}
                    field={field}
                    value={values[field.key]}
                    onChange={(value) =>
                      setValues((current) => ({
                        ...current,
                        [field.key]: value,
                        ...(schema.kind === 'development_config' && field.key === 'source_kind'
                          ? { source_id: undefined }
                          : {}),
                        ...(schema.kind === 'asset_entry' &&
                        field.key === 'object_kind' &&
                        current.object_kind !== value
                          ? { object_id: '' }
                          : {}),
                        ...(schema.kind === 'atomic_metric' &&
                        field.key === 'model_id' &&
                        current.model_id !== value
                          ? {
                              metadata_object_id: '',
                              grain_columns: [],
                              time_column: '',
                              dimensions: [],
                            }
                          : {}),
                        ...(schema.kind === 'atomic_metric' &&
                        field.key === 'metadata_object_id' &&
                        current.metadata_object_id !== value
                          ? { grain_columns: [], time_column: '', dimensions: [] }
                          : {}),
                        ...(schema.kind === 'metric_modifier' &&
                        field.key === 'metadata_object_id' &&
                        current.metadata_object_id !== value
                          ? { dimension: '' }
                          : {}),
                        ...(schema.kind === 'derived_metric' &&
                        field.key === 'atomic_metric_id' &&
                        current.atomic_metric_id !== value
                          ? { dimensions: [] }
                          : {}),
                      }))
                    }
                    disabled={mutation.isPending}
                    properties={values}
                  />
                </div>
              ))}
          </div>
          {error && <ErrorBanner message={error} />}
          <div className="flex justify-end gap-2 border-t border-[var(--color-border-default)] pt-4">
            <Button type="button" variant="outline" disabled={mutation.isPending} onClick={onClose}>
              {text.cancel}
            </Button>
            <Button type="submit" disabled={mutation.isPending}>
              {mutation.isPending ? text.loading : text.save}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}
