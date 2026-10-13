import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { Plus, Trash2, Database, History, LoaderCircle, PlugZap } from 'lucide-react';
import {
  ErrorBanner,
  FormInput,
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  StatusBadge,
} from '../components/ui';
import { DatasourceSkeleton } from '../components/skeletons';
import {
  useDatasources,
  useCreateDatasource,
  useDeleteDatasource,
  useDatasourceProbeHistory,
  useTestDatasource,
} from '../hooks/useApi';
import { datasourceSchema, getFirstError } from '../lib/validations';
import { useAuthStore } from '../stores/auth';

const DB_TYPES = [
  'MySQL',
  'PostgreSQL',
  'MongoDB',
  'Redis',
  'Kafka',
  'ClickHouse',
  'Oracle',
  'SQLServer',
  'SQLite',
  'DuckDB',
];

const DEFAULT_PORTS: Record<string, number> = {
  MySQL: 3306,
  PostgreSQL: 5432,
  MongoDB: 27017,
  Redis: 6379,
  Kafka: 9092,
  ClickHouse: 8123,
  Oracle: 1521,
  SQLServer: 1433,
  SQLite: 0,
  DuckDB: 4214,
};

export default function DatasourcesPage() {
  const { t } = useTranslation();
  const { data: datasources = [], isLoading, error } = useDatasources();
  const createDatasource = useCreateDatasource();
  const deleteDatasource = useDeleteDatasource();
  const testDatasource = useTestDatasource();
  const user = useAuthStore((state) => state.user);
  const canWrite = user?.permissions.includes('datasource:write') ?? false;

  const [showCreate, setShowCreate] = useState(false);
  const [historyDatasourceId, setHistoryDatasourceId] = useState<string>();
  const { data: probeHistory = [], isLoading: isHistoryLoading } =
    useDatasourceProbeHistory(historyDatasourceId);
  const [form, setForm] = useState({
    name: '',
    db_type: 'MySQL',
    host: '',
    port: 3306,
    username: '',
    password: '',
    database_name: '',
  });

  const fetchError = error ? t('errors.somethingWentWrong') : null;

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!canWrite) return;
    const validation = datasourceSchema.safeParse(form);
    if (!validation.success) {
      toast.error(getFirstError(validation) || t('errors.somethingWentWrong'));
      return;
    }
    createDatasource.mutate(validation.data, {
      onSuccess: () => {
        setShowCreate(false);
        setForm({
          name: '',
          db_type: 'MySQL',
          host: '',
          port: 3306,
          username: '',
          password: '',
          database_name: '',
        });
        toast.success(t('datasources.createSuccess'));
      },
      onError: () => {
        toast.error(t('errors.somethingWentWrong'));
      },
    });
  };

  const handleDelete = (id: string) => {
    if (!canWrite) return;
    if (!confirm(t('datasources.deleteConfirm'))) return;
    deleteDatasource.mutate(id, {
      onSuccess: () => toast.success(t('datasources.deleteSuccess')),
      onError: () => toast.error(t('errors.somethingWentWrong')),
    });
  };

  const handleTest = (id: string) => {
    if (!canWrite) return;
    testDatasource.mutate(id, {
      onSuccess: (result) => {
        if (!result.supported) {
          toast.warning(t('datasources.probeUnsupported'));
        } else if (result.status === 'healthy') {
          toast.success(t('datasources.probeSuccess', { latency: result.latency_ms ?? 0 }));
        } else {
          toast.error(t('datasources.probeFailed'));
        }
      },
      onError: () => toast.error(t('errors.somethingWentWrong')),
    });
  };

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-xl font-bold text-[var(--color-text-primary)]">
            {t('datasources.title')}
          </h1>
          <p className="mt-1 text-sm text-[var(--color-text-tertiary)]">
            {t('datasources.subtitle')}
          </p>
        </div>
        {canWrite && (
          <button
            onClick={() => setShowCreate(true)}
            className="inline-flex shrink-0 items-center gap-2 rounded-lg bg-[var(--color-action)] px-4 py-2.5 text-sm font-medium text-white hover:bg-[var(--color-action-hover)]"
          >
            <Plus size={16} /> {t('datasources.addDatasource')}
          </button>
        )}
      </div>

      {fetchError && <ErrorBanner message={fetchError} />}

      {/* Health summary */}
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
        <div className="data-panel p-4">
          <p className="text-xs text-[var(--color-text-tertiary)]">
            {t('datasources.totalConnections')}
          </p>
          <p className="mt-1 text-lg font-bold text-[var(--color-text-primary)]">
            {datasources.length}
          </p>
        </div>
        <div className="data-panel p-4">
          <p className="text-xs text-[var(--color-text-tertiary)]">
            {t('datasources.healthyConnections')}
          </p>
          <p className="mt-1 text-lg font-bold text-[var(--color-success)]">
            {datasources.filter((d) => d.health_status === 'healthy').length}
          </p>
        </div>
        <div className="data-panel p-4">
          <p className="text-xs text-[var(--color-text-tertiary)]">
            {t('datasources.unhealthyConnections')}
          </p>
          <p className="mt-1 text-lg font-bold text-[var(--color-error)]">
            {datasources.filter((d) => d.health_status === 'unhealthy').length}
          </p>
        </div>
      </div>

      {/* Datasource list */}
      {isLoading ? (
        <DatasourceSkeleton />
      ) : (
        <div className="data-panel overflow-hidden">
          <div className="overflow-x-auto">
            <table className="data-table min-w-[680px]" aria-label={t('datasources.title')}>
              <thead>
                <tr>
                  <th scope="col">{t('datasources.connectionName')}</th>
                  <th scope="col">{t('datasources.host')}</th>
                  <th scope="col">{t('datasources.databaseName')}</th>
                  <th scope="col">{t('datasources.healthStatus')}</th>
                  <th scope="col" className="text-right">
                    {t('common.actions')}
                  </th>
                </tr>
              </thead>
              <tbody>
                {datasources.map((ds) => (
                  <tr key={ds.id}>
                    <td>
                      <div className="flex items-center gap-2 font-medium text-[var(--color-text-primary)]">
                        <Database
                          size={16}
                          aria-hidden="true"
                          className="shrink-0 text-[var(--color-accent)]"
                        />
                        {ds.name}
                      </div>
                    </td>
                    <td className="whitespace-nowrap">
                      {ds.db_type === 'SQLite'
                        ? `${ds.db_type} · ${ds.host}`
                        : `${ds.db_type} @ ${ds.host}:${ds.port}`}
                    </td>
                    <td>{ds.database_name || '—'}</td>
                    <td>
                      <StatusBadge
                        status={ds.health_status}
                        label={t(`datasources.healthStatuses.${ds.health_status}`, {
                          defaultValue: ds.health_status,
                        })}
                      />
                      {ds.last_test_at && (
                        <div className="mt-1 text-xs text-[var(--color-text-tertiary)]">
                          {t('datasources.lastTested', {
                            time: new Date(ds.last_test_at).toLocaleString(),
                          })}
                        </div>
                      )}
                    </td>
                    <td className="text-right">
                      <div className="inline-flex items-center gap-1">
                        <button
                          type="button"
                          onClick={() => setHistoryDatasourceId(ds.id)}
                          aria-label={`${t('datasources.probeHistory')} ${ds.name}`}
                          className="inline-flex items-center gap-1.5 rounded-lg px-3 py-2 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-tertiary)]"
                        >
                          <History size={14} aria-hidden="true" />
                          {t('datasources.history')}
                        </button>
                        {canWrite && (
                          <button
                            type="button"
                            onClick={() => handleTest(ds.id)}
                            disabled={testDatasource.isPending}
                            aria-label={`${t('datasources.testConnection')} ${ds.name}`}
                            className="inline-flex items-center gap-1.5 rounded-lg px-3 py-2 text-xs text-[var(--color-action)] hover:bg-[var(--color-action-subtle)] disabled:opacity-50"
                          >
                            {testDatasource.isPending && testDatasource.variables === ds.id ? (
                              <LoaderCircle size={14} className="animate-spin" aria-hidden="true" />
                            ) : (
                              <PlugZap size={14} aria-hidden="true" />
                            )}
                            {t('datasources.testConnection')}
                          </button>
                        )}
                        {canWrite && (
                          <button
                            type="button"
                            onClick={() => handleDelete(ds.id)}
                            disabled={deleteDatasource.isPending}
                            aria-label={`${t('common.delete')} ${ds.name}`}
                            className="inline-flex items-center gap-1.5 rounded-lg px-3 py-2 text-xs text-[var(--color-error)] hover:bg-[var(--color-error-subtle)] disabled:opacity-50"
                          >
                            <Trash2 size={14} aria-hidden="true" />
                            {t('common.delete')}
                          </button>
                        )}
                      </div>
                    </td>
                  </tr>
                ))}
                {datasources.length === 0 && (
                  <tr>
                    <td colSpan={5} className="py-12 text-center text-[var(--color-text-tertiary)]">
                      {t('datasources.noDatasources')}
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {/* Create dialog */}
      <Dialog open={showCreate} onOpenChange={setShowCreate}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{t('datasources.addDatasource')}</DialogTitle>
          </DialogHeader>
          <form onSubmit={handleCreate} className="space-y-3">
            <FormInput
              placeholder={t('datasources.connectionName')}
              value={form.name}
              onChange={(v) => setForm({ ...form, name: v })}
              required
            />
            <Select
              value={form.db_type}
              onValueChange={(v) =>
                setForm({ ...form, db_type: v, port: DEFAULT_PORTS[v] ?? form.port })
              }
            >
              <SelectTrigger>
                <SelectValue placeholder={t('datasources.dbType')} />
              </SelectTrigger>
              <SelectContent>
                {DB_TYPES.map((t) => (
                  <SelectItem key={t} value={t}>
                    {t}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <div className="flex gap-2">
              <FormInput
                placeholder={
                  form.db_type === 'SQLite'
                    ? t('datasources.sqlitePath')
                    : form.db_type === 'DuckDB'
                      ? t('datasources.swanlakeHost')
                      : t('datasources.host')
                }
                value={form.host}
                onChange={(v) => setForm({ ...form, host: v })}
                required
                className="flex-1"
              />
              {form.db_type !== 'SQLite' && (
                <FormInput
                  type="number"
                  placeholder={t('datasources.port')}
                  value={form.port}
                  onChange={(v) => setForm({ ...form, port: Number(v) })}
                  required
                  className="w-24"
                />
              )}
            </div>
            {form.db_type !== 'SQLite' && form.db_type !== 'DuckDB' && (
              <>
                <FormInput
                  placeholder={t('datasources.username')}
                  value={form.username}
                  onChange={(v) => setForm({ ...form, username: v })}
                />
                <FormInput
                  type="password"
                  placeholder={t('datasources.password')}
                  value={form.password}
                  onChange={(v) => setForm({ ...form, password: v })}
                />
              </>
            )}
            {form.db_type !== 'SQLite' && (
              <FormInput
                placeholder={
                  form.db_type === 'DuckDB'
                    ? t('datasources.duckdbCatalog')
                    : t('datasources.databaseName')
                }
                value={form.database_name}
                onChange={(v) => setForm({ ...form, database_name: v })}
                required={form.db_type === 'DuckDB'}
              />
            )}
            <div className="flex gap-2 pt-2">
              <button
                type="button"
                onClick={() => setShowCreate(false)}
                className="flex-1 rounded-lg border border-[var(--color-border-hover)] px-4 py-2.5 text-sm text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-tertiary)]"
              >
                {t('common.cancel')}
              </button>
              <button
                type="submit"
                disabled={createDatasource.isPending}
                className="flex-1 rounded-lg bg-[var(--color-action)] px-4 py-2.5 text-sm font-medium text-white hover:bg-[var(--color-action-hover)] disabled:opacity-50"
              >
                {createDatasource.isPending ? t('common.loading') : t('datasources.add')}
              </button>
            </div>
          </form>
        </DialogContent>
      </Dialog>

      <Dialog
        open={!!historyDatasourceId}
        onOpenChange={(open) => !open && setHistoryDatasourceId(undefined)}
      >
        <DialogContent className="sm:max-w-2xl">
          <DialogHeader>
            <DialogTitle>{t('datasources.probeHistory')}</DialogTitle>
          </DialogHeader>
          {isHistoryLoading ? (
            <div className="flex justify-center py-10">
              <LoaderCircle className="animate-spin text-[var(--color-action)]" />
            </div>
          ) : probeHistory.length === 0 ? (
            <p className="py-10 text-center text-sm text-[var(--color-text-tertiary)]">
              {t('datasources.noProbeHistory')}
            </p>
          ) : (
            <div className="max-h-[420px] overflow-auto">
              <table className="data-table w-full">
                <thead>
                  <tr>
                    <th>{t('datasources.observedAt')}</th>
                    <th>{t('datasources.healthStatus')}</th>
                    <th>{t('datasources.latency')}</th>
                    <th>{t('datasources.probeDetail')}</th>
                  </tr>
                </thead>
                <tbody>
                  {probeHistory.map((probe) => (
                    <tr key={probe.id}>
                      <td className="whitespace-nowrap">
                        {new Date(probe.observed_at).toLocaleString()}
                      </td>
                      <td>
                        <StatusBadge
                          status={probe.status}
                          label={t(`datasources.healthStatuses.${probe.status}`)}
                        />
                      </td>
                      <td>{probe.latency_ms === null ? '—' : `${probe.latency_ms} ms`}</td>
                      <td>{probe.error_message ?? t('datasources.probePassed')}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </DialogContent>
      </Dialog>
    </div>
  );
}
