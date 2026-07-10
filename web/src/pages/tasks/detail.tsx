import { useEffect, useState } from 'react';
import { useParams, Link, useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { ArrowLeft, Pause, Play } from 'lucide-react';
import client from '../../api/client';
import MetricCard from '../../components/MetricCard';

interface TaskDetail {
  id: string;
  name: string;
  description: string | null;
  source_name: string;
  source_type: string;
  target_name: string;
  target_type: string;
  cron_expr: string | null;
  status: string;
  config_json: string;
  created_at: string;
  updated_at: string;
  recent_runs: Array<{
    id: string;
    status: string;
    start_time: string;
    end_time: string | null;
    rows_synced: number;
    bytes_synced: number;
    total_rows: number;
    error_message: string | null;
  }>;
}

export default function TaskDetailPage() {
  const { t } = useTranslation();
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const [task, setTask] = useState<TaskDetail | null>(null);
  const [loading, setLoading] = useState(true);

  const statusLabel: Record<string, string> = {
    draft: t('tasks.status.draft'),
    running: t('tasks.status.running'),
    paused: t('tasks.status.paused'),
    completed: t('tasks.status.completed'),
    failed: t('tasks.status.failed'),
    cancelled: t('tasks.status.cancelled'),
  };

  useEffect(() => {
    if (!id) return;
    const controller = new AbortController();
    client
      .get(`/tasks/${id}`, { signal: controller.signal })
      .then((res) => setTask(res.data))
      .catch((err) => {
        if (err?.name !== 'CanceledError' && err?.code !== 'ERR_CANCELED') {
          navigate('/tasks');
        }
      })
      .finally(() => setLoading(false));

    return () => controller.abort();
  }, [id, navigate]);

  const updateStatus = async (status: string) => {
    if (!id) return;
    await client.patch(`/tasks/${id}/status`, { status });
    const { data } = await client.get(`/tasks/${id}`);
    setTask(data);
  };

  if (loading) {
    return (
      <div className="flex justify-center py-20">
        <div className="h-8 w-8 animate-spin rounded-full border-2 border-zinc-600 border-t-blue-400" />
      </div>
    );
  }

  if (!task) return null;

  const progress = task.recent_runs.length > 0
    ? (task.recent_runs[0].total_rows > 0
        ? ((task.recent_runs[0].rows_synced / task.recent_runs[0].total_rows) * 100).toFixed(0)
        : '0')
    : null;

  return (
    <div className="space-y-6">
      {/* Breadcrumb + header */}
      <div className="flex items-center gap-4">
        <Link to="/tasks" className="text-zinc-500 hover:text-zinc-300">
          <ArrowLeft size={20} />
        </Link>
        <div className="flex-1">
          <h1 className="text-xl font-bold text-zinc-100">{task.name}</h1>
          <div className="mt-1 flex items-center gap-3">
            <span className="text-xs text-zinc-500">ID: {task.id.slice(0, 8)}...</span>
            <span className="text-xs text-zinc-600">
              {task.source_type} → {task.target_type}
            </span>
            {task.cron_expr && (
              <span className="rounded-full bg-zinc-800 px-2 py-0.5 text-xs text-zinc-400">
                {task.cron_expr}
              </span>
            )}
          </div>
        </div>
        <div className="flex gap-2">
          {task.status === 'running' ? (
            <button
              onClick={() => updateStatus('paused')}
              className="flex items-center gap-1.5 rounded-xl border border-amber-500/30 px-3 py-2 text-sm text-amber-400 hover:bg-amber-500/10"
            >
              <Pause size={16} /> {t('tasks.detail.pause')}
            </button>
          ) : task.status === 'draft' || task.status === 'paused' ? (
            <button
              onClick={() => updateStatus('running')}
              className="flex items-center gap-1.5 rounded-xl bg-emerald-500 px-3 py-2 text-sm font-medium text-white hover:bg-emerald-600"
            >
              <Play size={16} /> {t('tasks.detail.start')}
            </button>
          ) : null}
        </div>
      </div>

      {/* Progress (if running) */}
      {task.recent_runs.length > 0 && task.recent_runs[0].status === 'running' && (
        <div className="rounded-2xl border border-zinc-800/60 bg-zinc-900/40 p-5">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-zinc-300">{t('tasks.detail.realTimeProgress')}</h3>
            <span className="text-sm text-zinc-400">{progress}%</span>
          </div>
          <div className="mt-3 h-2 overflow-hidden rounded-full bg-zinc-800">
            <div
              className="h-full rounded-full bg-gradient-to-r from-blue-500 to-emerald-400 transition-all duration-1000"
              style={{ width: `${progress}%` }}
            />
          </div>
        </div>
      )}

      {/* Metrics */}
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
        <MetricCard
          title={t('tasks.detail.source')}
          value={task.source_name}
          subtitle={task.source_type}
        />
        <MetricCard
          title={t('tasks.detail.target')}
          value={task.target_name}
          subtitle={task.target_type}
        />
        <MetricCard
          title={t('tasks.detail.status')}
          value={statusLabel[task.status] || task.status}
          subtitle={`${t('tasks.detail.createdAt')} ${task.created_at}`}
        />
      </div>

      {/* Execution history */}
      <div className="rounded-2xl border border-zinc-800/60 bg-zinc-900/40 p-5 backdrop-blur-sm">
        <h3 className="mb-4 text-sm font-medium text-zinc-300">{t('tasks.detail.executionHistory')}</h3>
        {task.recent_runs.length === 0 ? (
          <p className="text-sm text-zinc-600">{t('tasks.detail.noExecutionRecords')}</p>
        ) : (
          <div className="space-y-2">
            {task.recent_runs.map((run) => (
              <div
                key={run.id}
                className="flex items-center justify-between rounded-xl border border-zinc-800/40 bg-zinc-900/20 p-3"
              >
                <div>
                  <p className="text-xs font-mono text-zinc-400">#{run.id.slice(0, 8)}</p>
                  <p className="text-xs text-zinc-500">{run.start_time}</p>
                </div>
                <div className="text-right">
                  <p className="text-xs text-zinc-300">
                    {t('tasks.detail.rowsSynced', { count: (run.rows_synced ?? 0).toLocaleString() })}
                  </p>
                  {run.error_message && (
                    <p className="mt-0.5 text-xs text-rose-400">{run.error_message.slice(0, 80)}</p>
                  )}
                </div>
                <span
                  className={`rounded-full px-2 py-0.5 text-xs ${
                    run.status === 'completed'
                      ? 'bg-emerald-500/10 text-emerald-400'
                      : run.status === 'running'
                        ? 'bg-blue-500/10 text-blue-400'
                        : run.status === 'failed'
                          ? 'bg-rose-500/10 text-rose-400'
                          : 'bg-zinc-500/10 text-zinc-400'
                  }`}
                >
                  {statusLabel[run.status] || run.status}
                </span>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
