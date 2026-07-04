import { useEffect, useState } from 'react';
import { Link } from 'react-router-dom';
import { Database, CheckCircle, AlertTriangle, Activity } from 'lucide-react';
import MetricCard from '../components/MetricCard';
import LogConsole from '../components/LogConsole';
import client from '../api/client';

interface Overview {
  total_tasks: number;
  running_tasks: number;
  failed_tasks: number;
  total_datasources: number;
  healthy_datasources: number;
  today_synced_rows: number;
  today_synced_bytes: number;
  success_rate: number;
  recent_runs: Array<{
    id: string;
    task_id: string;
    task_name: string;
    status: string;
    start_time: string;
    rows_synced: number;
    bytes_synced: number;
  }>;
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
}

function formatNumber(n: number): string {
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + 'M';
  if (n >= 1_000) return (n / 1_000).toFixed(1) + 'K';
  return n.toString();
}

export default function DashboardPage() {
  const [overview, setOverview] = useState<Overview | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    client
      .get('/monitoring/overview')
      .then((res) => setOverview(res.data))
      .catch(console.error)
      .finally(() => setLoading(false));
  }, []);

  if (loading) {
    return (
      <div className="flex items-center justify-center py-20">
        <div className="h-8 w-8 animate-spin rounded-full border-2 border-zinc-600 border-t-blue-400" />
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Page header */}
      <div>
        <h1 className="text-xl font-bold text-zinc-100">仪表盘概览</h1>
        <p className="mt-1 text-sm text-zinc-500">系统运行状态与关键指标</p>
      </div>

      {/* KPI cards */}
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <MetricCard
          title="同步任务总数"
          value={overview?.total_tasks ?? 0}
          subtitle={`${overview?.running_tasks ?? 0} 个运行中`}
          icon={<Activity size={18} />}
        />
        <MetricCard
          title="今日同步数据"
          value={formatBytes(overview?.today_synced_bytes ?? 0)}
          subtitle={`${formatNumber(overview?.today_synced_rows ?? 0)} 行`}
          trend="up"
        />
        <MetricCard
          title="成功率"
          value={`${(overview?.success_rate ?? 100).toFixed(1)}%`}
          subtitle={`${overview?.failed_tasks ?? 0} 个失败任务`}
          trend={overview && overview.failed_tasks > 0 ? 'down' : 'up'}
          icon={<CheckCircle size={18} />}
        />
        <MetricCard
          title="健康数据源"
          value={`${overview?.healthy_datasources ?? 0}/${overview?.total_datasources ?? 0}`}
          subtitle="连接正常"
          icon={<Database size={18} />}
        />
      </div>

      {/* Recent runs + Log */}
      <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
        {/* Recent task runs */}
        <div className="rounded-2xl border border-zinc-800/60 bg-zinc-900/40 p-5 backdrop-blur-sm">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-zinc-300">最近执行记录</h3>
            <Link to="/tasks" className="text-xs text-blue-400 hover:text-blue-300">
              查看全部 →
            </Link>
          </div>
          <div className="mt-4 space-y-3">
            {overview?.recent_runs?.length === 0 ? (
              <p className="text-sm text-zinc-600">暂无执行记录</p>
            ) : (
              overview?.recent_runs?.slice(0, 5).map((run) => (
                <div
                  key={run.id}
                  className="flex items-center justify-between rounded-xl border border-zinc-800/40 bg-zinc-900/20 p-3"
                >
                  <div>
                    <Link
                      to={`/tasks/${run.task_id}`}
                      className="text-sm font-medium text-zinc-200 hover:text-blue-400"
                    >
                      {run.task_name}
                    </Link>
                    <p className="text-xs text-zinc-500">{run.start_time}</p>
                  </div>
                  <div className="text-right">
                    <span
                      className={`inline-block rounded-full px-2 py-0.5 text-xs font-medium ${
                        run.status === 'completed'
                          ? 'bg-emerald-500/10 text-emerald-400'
                          : run.status === 'running'
                            ? 'bg-blue-500/10 text-blue-400'
                            : run.status === 'failed'
                              ? 'bg-rose-500/10 text-rose-400'
                              : 'bg-zinc-500/10 text-zinc-400'
                      }`}
                    >
                      {run.status}
                    </span>
                    <p className="mt-1 text-xs text-zinc-500">
                      {formatNumber(run.rows_synced)} 行
                    </p>
                  </div>
                </div>
              ))
            )}
          </div>
        </div>

        {/* Data flow visualization placeholder */}
        <div className="rounded-2xl border border-zinc-800/60 bg-zinc-900/40 p-5 backdrop-blur-sm">
          <h3 className="text-sm font-medium text-zinc-300">数据流可视化</h3>
          <div className="mt-4 flex h-48 items-center justify-center rounded-xl border border-dashed border-zinc-700 bg-zinc-900/30">
            <div className="text-center">
              <Activity size={32} className="mx-auto text-zinc-600" />
              <p className="mt-2 text-sm text-zinc-600">
                实时数据流拓扑图
              </p>
              <p className="text-xs text-zinc-700">集成 ape-dts 引擎后启用</p>
            </div>
          </div>
        </div>
      </div>

      {/* Log console placeholder */}
      <LogConsole
        title="系统日志"
        logs={[
          { timestamp: new Date().toISOString(), level: 'INFO', component: 'system', message: 'DTS Platform 已启动' },
          { timestamp: new Date().toISOString(), level: 'INFO', component: 'db', message: '数据库连接成功: SQLite' },
        ]}
      />
    </div>
  );
}
