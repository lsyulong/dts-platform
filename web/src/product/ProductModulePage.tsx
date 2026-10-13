import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Plus, Download, RefreshCw, Search } from 'lucide-react';
import { ErrorBanner } from '../components/ui';
import { Button } from '../components/ui/button';
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '../components/ui/dialog';
import { apiErrorMessage } from '../lib/apiError';
import { captureSession, isCurrentSession } from '../lib/session';
import { useAuthStore } from '../stores/auth';
import type { ProductModule } from './catalog-types';
import { useProductResources, useResourceMutation, useResourceSchemas } from './api';
import type { ProductResource, ResourceSchema, ResourceActionResult } from './resource-types';
import ResourceEditor from './ResourceEditor';
import ResourceDetails from './ResourceDetails';
import ProductWorkspace from './ProductWorkspace';
import { localized, productText, statusText } from './ui-text';
import ResourceOperationDialog from './ResourceOperationDialog';
import ResourceActionOutcome from './ResourceActionOutcome';
import {
  initialActionPayload,
  canUseResourceAction,
  type PendingResourceAction,
} from './action-models';

function exportPage(records: ProductResource[], module: ProductModule, language: string) {
  const text = productText(language);
  const csvCell = (value: string) => `"${value.replace(/^[=+\-@]/, "'$&").replaceAll('"', '""')}"`;
  const csv = [
    [text.code, text.name, text.status, text.modified],
    ...records.map((record) => [
      record.code,
      record.name,
      statusText(record.status, language),
      record.updated_at,
    ]),
  ]
    .map((row) => row.map(csvCell).join(','))
    .join('\r\n');
  const url = URL.createObjectURL(new Blob(['\ufeff' + csv], { type: 'text/csv;charset=utf-8' }));
  const link = document.createElement('a');
  link.href = url;
  link.download = `${module.id}.csv`;
  link.click();
  URL.revokeObjectURL(url);
}

interface ProductModulePageProps {
  module: ProductModule;
  onActionNavigate?: (record: ProductResource, action: string) => boolean;
}

export default function ProductModulePage({ module, onActionNavigate }: ProductModulePageProps) {
  const identity = useAuthStore((state) =>
    JSON.stringify([state.user?.tenant_id, state.user?.id, state.token, state.user?.permissions]),
  );
  return (
    <ProductModuleContent
      key={`${identity}:${module.id}`}
      module={module}
      onActionNavigate={onActionNavigate}
    />
  );
}

function ProductModuleContent({ module, onActionNavigate }: ProductModulePageProps) {
  const { i18n } = useTranslation();
  const text = productText(i18n.language);
  const user = useAuthStore((state) => state.user);
  const [search, setSearch] = useState('');
  const [q, setQ] = useState('');
  const [status, setStatus] = useState('');
  const [offset, setOffset] = useState(0);
  const [editor, setEditor] = useState<{ record?: ProductResource } | null>(null);
  const [details, setDetails] = useState<ProductResource | null>(null);
  const [deletion, setDeletion] = useState<ProductResource | null>(null);
  const [action, setAction] = useState<PendingResourceAction | null>(null);
  const [outcome, setOutcome] = useState<ResourceActionResult | null>(null);
  const [error, setError] = useState('');
  const [exporting, setExporting] = useState(false);
  const definitions = useResourceSchemas();
  const schema = definitions.data?.find((definition) => definition.kind === module.resourceKind);
  const fallback: ResourceSchema = {
    kind: module.resourceKind ?? '',
    fields: module.fields ?? [],
    actions: [],
    statuses: [],
    read_permission: '',
    write_permission: '',
  };
  const canRead = !!schema && !!user?.permissions.includes(schema.read_permission);
  const canWrite = !!schema && !!user?.permissions.includes(schema.write_permission);
  const resources = useProductResources(
    module.resourceKind ?? '',
    { q, status, limit: 20, offset },
    canRead,
  );
  const mutation = useResourceMutation(module.resourceKind ?? '');
  const records = canRead && !resources.error ? (resources.data?.data ?? []) : [];

  const downloadCurrentPage = async () => {
    if (!canRead || exporting || resources.isFetching) return;
    const session = captureSession();
    const permissions = JSON.stringify(user?.permissions ?? []);
    setExporting(true);
    setError('');
    try {
      const current = await resources.refetch();
      if (current.error) throw current.error;
      if (
        isCurrentSession(session) &&
        permissions === JSON.stringify(useAuthStore.getState().user?.permissions ?? []) &&
        current.isSuccess &&
        current.data
      ) {
        exportPage(current.data.data, module, i18n.language);
      }
    } catch (caught) {
      if (isCurrentSession(session)) setError(apiErrorMessage(caught, text.error));
    } finally {
      if (isCurrentSession(session)) setExporting(false);
    }
  };

  const commitOperation = async () => {
    if (
      mutation.isPending ||
      !canRead ||
      (deletion
        ? !canWrite
        : !action ||
          !canUseResourceAction(
            action.record.kind,
            action.name,
            canRead,
            canWrite,
            user?.permissions ?? [],
          ))
    )
      return;
    const session = captureSession();
    setError('');
    try {
      if (deletion)
        await mutation.mutateAsync({ type: 'delete', id: deletion.id, version: deletion.version });
      else if (action) {
        const result = await mutation.mutateAsync({
          type: 'action',
          id: action.record.id,
          version: action.record.version,
          action: action.name,
          payload: action.payload,
        });
        if (isCurrentSession(session) && result && 'resource' in result) setOutcome(result);
      }
      if (isCurrentSession(session)) {
        setDeletion(null);
        setAction(null);
      }
    } catch (caught) {
      if (isCurrentSession(session)) setError(apiErrorMessage(caught, text.error));
    }
  };

  return (
    <div className="space-y-5">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h1 className="text-xl font-semibold">{localized(module.title, i18n.language)}</h1>
          <p className="mt-1 text-sm text-[var(--color-text-tertiary)]">
            {localized(module.description, i18n.language)}
          </p>
        </div>
        <div className="flex gap-2">
          <Button
            variant="outline"
            disabled={!canRead || !records.length || resources.isFetching || exporting}
            onClick={() => void downloadCurrentPage()}
          >
            <Download size={15} />
            {text.exportPage}
          </Button>
          <Button
            disabled={!canWrite || !schema?.actions.includes('create')}
            onClick={() => setEditor({})}
          >
            <Plus size={16} />
            {text.create}
          </Button>
        </div>
      </div>
      {definitions.error && (
        <ErrorBanner message={apiErrorMessage(definitions.error, text.schemaUnavailable)} />
      )}
      {schema && !canRead && <ErrorBanner message={text.forbidden} />}
      {error && <ErrorBanner message={error} />}
      <section className="data-panel">
        <form
          className="data-toolbar"
          onSubmit={(event) => {
            event.preventDefault();
            setQ(search.trim());
            setOffset(0);
          }}
        >
          <div className="flex flex-wrap items-center gap-2">
            <label className="relative">
              <Search
                size={15}
                className="absolute left-3 top-2.5 text-[var(--color-text-tertiary)]"
              />
              <input
                aria-label={text.search}
                placeholder={text.search}
                value={search}
                onChange={(event) => setSearch(event.target.value)}
                className="w-60 rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] py-2 pl-9 pr-3 text-sm"
              />
            </label>
            <select
              aria-label={text.status}
              value={status}
              onChange={(event) => {
                setStatus(event.target.value);
                setOffset(0);
              }}
              className="rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] px-3 py-2 text-sm"
            >
              <option value="">{text.allStatus}</option>
              {schema?.statuses.map((value) => (
                <option key={value} value={value}>
                  {statusText(value, i18n.language)}
                </option>
              ))}
            </select>
            <Button type="submit" variant="outline">
              {text.query}
            </Button>
            <Button
              type="button"
              variant="ghost"
              onClick={() => {
                setSearch('');
                setQ('');
                setStatus('');
                setOffset(0);
              }}
            >
              {text.reset}
            </Button>
          </div>
          <Button
            type="button"
            variant="ghost"
            aria-label={text.refresh}
            disabled={!canRead || resources.isFetching}
            onClick={() => void resources.refetch()}
          >
            <RefreshCw size={15} />
          </Button>
        </form>
        {resources.error && (
          <div className="p-4">
            <ErrorBanner message={apiErrorMessage(resources.error, text.error)} />
          </div>
        )}
        <ProductWorkspace
          module={module}
          schema={schema ?? fallback}
          records={records}
          canWrite={canWrite}
          status={status}
          onStatusChange={(value) => {
            setStatus(value);
            setOffset(0);
          }}
          onView={setDetails}
          onEdit={(record) => setEditor({ record })}
          onDelete={setDeletion}
          onAction={(record, name, payload) => {
            if (onActionNavigate?.(record, name)) return;
            setAction({
              record,
              name,
              payload: { ...initialActionPayload(record, name), ...payload },
            });
            setError('');
          }}
          busy={mutation.isPending}
        />
        {resources.isSuccess && !resources.isFetching && !definitions.error && !records.length && (
          <div className="px-5 py-14 text-center">
            <p className="font-medium">
              {definitions.isPending || (canRead && resources.isPending)
                ? text.loading
                : !schema
                  ? text.schemaUnavailable
                  : text.emptyTitle}
            </p>
            <p className="mt-2 text-sm text-[var(--color-text-tertiary)]">
              {schema ? text.emptyDescription : text.schemaDescription}
            </p>
          </div>
        )}
        <div className="flex flex-wrap items-center justify-between gap-3 border-t border-[var(--color-border-default)] px-4 py-3 text-sm">
          <span className="text-[var(--color-text-tertiary)]">
            {canRead && !resources.error && resources.data
              ? `${resources.data.total} ${text.total}`
              : '—'}
          </span>
          <div className="flex items-center gap-2">
            <Button
              variant="outline"
              disabled={!offset || resources.isFetching}
              onClick={() => setOffset(Math.max(0, offset - 20))}
            >
              {text.previous}
            </Button>
            <span>{Math.floor(offset / 20) + 1}</span>
            <Button
              variant="outline"
              disabled={
                !resources.data || offset + 20 >= resources.data.total || resources.isFetching
              }
              onClick={() => setOffset(offset + 20)}
            >
              {text.next}
            </Button>
          </div>
        </div>
      </section>
      {canWrite && !resources.error && editor && schema && (
        <ResourceEditor
          key={editor.record?.id ?? 'new'}
          module={module}
          schema={schema}
          existing={editor.record}
          onClose={() => setEditor(null)}
        />
      )}
      {canRead && !resources.error && details && schema && (
        <ResourceDetails
          module={module}
          schema={schema}
          record={details}
          onClose={() => setDetails(null)}
        />
      )}
      <Dialog
        open={canRead && !resources.error && !!outcome}
        onOpenChange={(open) => {
          if (!open) setOutcome(null);
        }}
      >
        <DialogContent className="max-w-3xl">
          <DialogTitle>{text.result}</DialogTitle>
          <DialogDescription>{outcome?.message ?? outcome?.resource.name}</DialogDescription>
          {outcome && <ResourceActionOutcome outcome={outcome} />}
        </DialogContent>
      </Dialog>
      <ResourceOperationDialog
        deletion={canWrite && !resources.error ? deletion : null}
        action={
          action &&
          !resources.error &&
          canUseResourceAction(
            action.record.kind,
            action.name,
            canRead,
            canWrite,
            user?.permissions ?? [],
          )
            ? action
            : null
        }
        busy={mutation.isPending}
        error={error}
        onChange={(payload) => setAction((current) => (current ? { ...current, payload } : null))}
        onClose={() => {
          setDeletion(null);
          setAction(null);
          setError('');
        }}
        onCommit={() => void commitOperation()}
      />
    </div>
  );
}
