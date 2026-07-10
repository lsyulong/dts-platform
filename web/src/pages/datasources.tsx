import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Plus, Trash2, Wifi, WifiOff } from 'lucide-react';
import client from '../api/client';

interface Datasource {
  id: string;
  name: string;
  db_type: string;
  host: string;
  port: number;
  username: string | null;
  database_name: string | null;
  health_status: string;
  created_at: string;
}

const DB_TYPES = ['MySQL', 'PostgreSQL', 'MongoDB', 'Redis', 'Kafka', 'ClickHouse', 'Oracle', 'SQLServer'];

const dbIcons: Record<string, string> = {
  MySQL: '🔵', PostgreSQL: '🐘', MongoDB: '🍃', Redis: '🔴',
  Kafka: '📨', ClickHouse: '⚡', Oracle: '🔶', SQLServer: '🟢',
};

export default function DatasourcesPage() {
  const { t } = useTranslation();
  const [datasources, setDatasources] = useState<Datasource[]>([]);
  const [loading, setLoading] = useState(true);
  const [fetchError, setFetchError] = useState<string | null>(null);
  const [showCreate, setShowCreate] = useState(false);
  const [form, setForm] = useState({
    name: '', db_type: 'MySQL', host: '', port: 3306,
    username: '', password: '', database_name: '',
  });

  const loadData = async () => {
    try {
      const { data } = await client.get('/datasources');
      setDatasources(data.data || []);
      setFetchError(null);
    } catch {
      setFetchError(t('errors.somethingWentWrong'));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => { loadData(); }, []);

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault();
    try {
      await client.post('/datasources', form);
      setShowCreate(false);
      setForm({ name: '', db_type: 'MySQL', host: '', port: 3306, username: '', password: '', database_name: '' });
      loadData();
    } catch {
      setFetchError(t('errors.somethingWentWrong'));
    }
  };

  const handleDelete = async (id: string) => {
    if (!confirm(t('datasources.deleteConfirm'))) return;
    try {
      await client.delete(`/datasources/${id}`);
      loadData();
    } catch {
      setFetchError(t('errors.somethingWentWrong'));
    }
  };

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-xl font-bold text-zinc-100">{t('datasources.title')}</h1>
          <p className="mt-1 text-sm text-zinc-500">{t('datasources.subtitle')}</p>
        </div>
        <button
          onClick={() => setShowCreate(true)}
          className="flex items-center gap-2 rounded-xl bg-blue-500 px-4 py-2.5 text-sm font-medium text-white shadow-[0_0_20px_rgba(37,99,235,0.2)] hover:bg-blue-600"
        >
          <Plus size={16} /> {t('datasources.addDatasource')}
        </button>
      </div>

      {/* Error banner */}
      {fetchError && (
        <div className="rounded-lg border border-red-500/20 bg-red-500/10 px-4 py-2.5 text-sm text-red-400">
          {fetchError}
        </div>
      )}

      {/* Health summary */}
      <div className="grid grid-cols-3 gap-4">
        <div className="rounded-xl border border-zinc-800/60 bg-zinc-900/40 p-4">
          <p className="text-xs text-zinc-500">{t('datasources.totalConnections')}</p>
          <p className="mt-1 text-lg font-bold text-zinc-100">{datasources.length}</p>
        </div>
        <div className="rounded-xl border border-zinc-800/60 bg-zinc-900/40 p-4">
          <p className="text-xs text-zinc-500">{t('datasources.healthyConnections')}</p>
          <p className="mt-1 text-lg font-bold text-emerald-400">
            {datasources.filter((d) => d.health_status === 'healthy').length}
          </p>
        </div>
        <div className="rounded-xl border border-zinc-800/60 bg-zinc-900/40 p-4">
          <p className="text-xs text-zinc-500">{t('datasources.unhealthyConnections')}</p>
          <p className="mt-1 text-lg font-bold text-rose-400">
            {datasources.filter((d) => d.health_status === 'unhealthy').length}
          </p>
        </div>
      </div>

      {/* Datasource list */}
      {loading ? (
        <div className="flex justify-center py-10">
          <div className="h-8 w-8 animate-spin rounded-full border-2 border-zinc-600 border-t-blue-400" />
        </div>
      ) : (
        <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
          {datasources.map((ds) => (
            <div
              key={ds.id}
              className="group rounded-2xl border border-zinc-800/60 bg-zinc-900/40 p-4 transition-all hover:border-zinc-700/60"
            >
              <div className="flex items-start justify-between">
                <div className="flex items-center gap-3">
                  <div className="flex h-10 w-10 items-center justify-center rounded-xl bg-zinc-800 text-lg">
                    {dbIcons[ds.db_type] || '🗄️'}
                  </div>
                  <div>
                    <p className="text-sm font-medium text-zinc-200">{ds.name}</p>
                    <p className="text-xs text-zinc-500">
                      {ds.db_type} @ {ds.host}:{ds.port}
                    </p>
                    {ds.database_name && (
                      <p className="text-xs text-zinc-600">DB: {ds.database_name}</p>
                    )}
                  </div>
                </div>
                <div className="flex items-center gap-2">
                  {ds.health_status === 'healthy' ? (
                    <Wifi size={14} className="text-emerald-400" />
                  ) : (
                    <WifiOff size={14} className="text-zinc-600" />
                  )}
                  <button
                    onClick={() => handleDelete(ds.id)}
                    className="rounded-lg p-1.5 text-zinc-600 opacity-0 transition-opacity hover:bg-rose-500/10 hover:text-rose-400 group-hover:opacity-100"
                  >
                    <Trash2 size={14} />
                  </button>
                </div>
              </div>
            </div>
          ))}
        </div>
      )}

      {/* Create modal */}
      {showCreate && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
          <div className="w-full max-w-md rounded-2xl border border-zinc-800 bg-zinc-900 p-6">
            <h2 className="text-lg font-bold text-zinc-100">{t('datasources.addDatasource')}</h2>
            <form onSubmit={handleCreate} className="mt-4 space-y-3">
              <input
                type="text" placeholder={t('datasources.connectionName')} required
                value={form.name} onChange={(e) => setForm({ ...form, name: e.target.value })}
                className="w-full rounded-xl border border-zinc-700 bg-zinc-800/50 px-4 py-2.5 text-sm text-zinc-200 placeholder-zinc-600 focus:border-blue-500 focus:outline-none"
              />
              <select
                value={form.db_type} onChange={(e) => setForm({ ...form, db_type: e.target.value })}
                className="w-full rounded-xl border border-zinc-700 bg-zinc-800/50 px-4 py-2.5 text-sm text-zinc-200"
              >
                {DB_TYPES.map((t) => (
                  <option key={t} value={t}>{t}</option>
                ))}
              </select>
              <div className="flex gap-2">
                <input
                  type="text" placeholder={t('datasources.host')} required
                  value={form.host} onChange={(e) => setForm({ ...form, host: e.target.value })}
                  className="flex-1 rounded-xl border border-zinc-700 bg-zinc-800/50 px-4 py-2.5 text-sm text-zinc-200 placeholder-zinc-600 focus:border-blue-500 focus:outline-none"
                />
                <input
                  type="number" placeholder={t('datasources.port')} required
                  value={form.port} onChange={(e) => setForm({ ...form, port: Number(e.target.value) })}
                  className="w-24 rounded-xl border border-zinc-700 bg-zinc-800/50 px-4 py-2.5 text-sm text-zinc-200"
                />
              </div>
              <input
                type="text" placeholder={t('datasources.username')}
                value={form.username} onChange={(e) => setForm({ ...form, username: e.target.value })}
                className="w-full rounded-xl border border-zinc-700 bg-zinc-800/50 px-4 py-2.5 text-sm text-zinc-200 placeholder-zinc-600 focus:border-blue-500 focus:outline-none"
              />
              <input
                type="password" placeholder={t('datasources.password')}
                value={form.password} onChange={(e) => setForm({ ...form, password: e.target.value })}
                className="w-full rounded-xl border border-zinc-700 bg-zinc-800/50 px-4 py-2.5 text-sm text-zinc-200 placeholder-zinc-600 focus:border-blue-500 focus:outline-none"
              />
              <input
                type="text" placeholder={t('datasources.databaseName')}
                value={form.database_name} onChange={(e) => setForm({ ...form, database_name: e.target.value })}
                className="w-full rounded-xl border border-zinc-700 bg-zinc-800/50 px-4 py-2.5 text-sm text-zinc-200 placeholder-zinc-600 focus:border-blue-500 focus:outline-none"
              />
              <div className="flex gap-2 pt-2">
                <button
                  type="button" onClick={() => setShowCreate(false)}
                  className="flex-1 rounded-xl border border-zinc-700 px-4 py-2.5 text-sm text-zinc-400 hover:bg-zinc-800"
                >
                  {t('common.cancel')}
                </button>
                <button
                  type="submit"
                  className="flex-1 rounded-xl bg-blue-500 px-4 py-2.5 text-sm font-medium text-white hover:bg-blue-600"
                >
                  {t('datasources.add')}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  );
}
