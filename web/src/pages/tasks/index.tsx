import { useEffect, useState } from 'react';
import { Link } from 'react-router-dom';
import { Plus, Search, Play, Pause, Trash2 } from 'lucide-react';
import client from '../../api/client';

interface Task {
  id: string;
  name: string;
  description: string | null;
  source_id: string;
  target_id: string;
  cron_expr: string | null;
  status: string;
  created_at: string;
}

const statusBadge: Record<string, string> = {
  draft: 'bg-zinc-500/10 text-zinc-400',
  running: 'bg-blue-500/10 text-blue-400',
  paused: 'bg-amber-500/10 text-amber-400',
  completed: 'bg-emerald-500/10 text-emerald-400',
  failed: 'bg-rose-500/10 text-rose-400',
};

const statusLabel: Record<string, string> = {
  draft: '草稿',
  running: '运行中',
  paused: '已暂停',
  completed: '已完成',
  failed: '失败',
};

export default function TaskListPage() {
  const [tasks, setTasks] = useState<Task[]>([]);
  const [search, setSearch] = useState('');
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    loadTasks();
  }, []);

  const loadTasks = async () => {
    try {
      const { data } = await client.get('/tasks');
      setTasks(data.data || []);
    } catch (err) {
      console.error('加载任务列表失败:', err);
    } finally {
      setLoading(false);
    }
  };

  const updateStatus = async (id: string, status: string) => {
    try {
      await client.patch(`/tasks/${id}/status`, { status });
      loadTasks();
    } catch (err) {
      console.error('更新任务状态失败:', err);
    }
  };

  const filteredTasks = tasks.filter(
    (t) =>
      t.name.toLowerCase().includes(search.toLowerCase()) ||
      (t.description || '').toLowerCase().includes(search.toLowerCase())
  );

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-xl font-bold text-zinc-100">任务管理</h1>
          <p className="mt-1 text-sm text-zinc-500">管理所有数据同步任务</p>
        </div>
        <Link
          to="/tasks/create"
          className="flex items-center gap-2 rounded-xl bg-blue-500 px-4 py-2.5 text-sm font-medium text-white shadow-[0_0_20px_rgba(37,99,235,0.2)] transition-all hover:bg-blue-600"
        >
          <Plus size={16} />
          创建任务
        </Link>
      </div>

      {/* Search */}
      <div className="relative">
        <Search size={16} className="absolute left-3 top-1/2 -translate-y-1/2 text-zinc-500" />
        <input
          type="text"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder="搜索任务名称或描述... (Ctrl+K)"
          className="w-full rounded-xl border border-zinc-700 bg-zinc-800/50 py-2.5 pl-10 pr-4 text-sm text-zinc-200 placeholder-zinc-600 focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
        />
      </div>

      {/* Task list */}
      {loading ? (
        <div className="flex justify-center py-10">
          <div className="h-8 w-8 animate-spin rounded-full border-2 border-zinc-600 border-t-blue-400" />
        </div>
      ) : filteredTasks.length === 0 ? (
        <div className="flex flex-col items-center justify-center rounded-2xl border border-dashed border-zinc-700 bg-zinc-900/30 py-16">
          <p className="text-sm text-zinc-500">暂无同步任务</p>
          <Link
            to="/tasks/create"
            className="mt-3 text-sm text-blue-400 hover:text-blue-300"
          >
            创建第一个任务 →
          </Link>
        </div>
      ) : (
        <div className="space-y-3">
          {filteredTasks.map((task) => (
            <div
              key={task.id}
              className="group flex items-center justify-between rounded-2xl border border-zinc-800/60 bg-zinc-900/40 p-4 transition-all hover:border-zinc-700/60"
            >
              <div className="flex-1">
                <Link
                  to={`/tasks/${task.id}`}
                  className="text-sm font-medium text-zinc-200 hover:text-blue-400"
                >
                  {task.name}
                </Link>
                <div className="mt-1 flex items-center gap-3">
                  <span
                    className={`inline-block rounded-full px-2 py-0.5 text-xs font-medium ${statusBadge[task.status] || 'text-zinc-400'}`}
                  >
                    {statusLabel[task.status] || task.status}
                  </span>
                  {task.cron_expr && (
                    <span className="text-xs text-zinc-600">cron: {task.cron_expr}</span>
                  )}
                </div>
                {task.description && (
                  <p className="mt-1 text-xs text-zinc-500">{task.description}</p>
                )}
              </div>

              {/* Action buttons (visible on hover) */}
              <div className="flex items-center gap-2 opacity-0 transition-opacity group-hover:opacity-100">
                {task.status === 'draft' || task.status === 'paused' ? (
                  <button
                    onClick={() => updateStatus(task.id, 'running')}
                    className="rounded-lg p-2 text-emerald-400 hover:bg-emerald-500/10"
                    title="启动"
                  >
                    <Play size={16} />
                  </button>
                ) : null}
                {task.status === 'running' ? (
                  <button
                    onClick={() => updateStatus(task.id, 'paused')}
                    className="rounded-lg p-2 text-amber-400 hover:bg-amber-500/10"
                    title="暂停"
                  >
                    <Pause size={16} />
                  </button>
                ) : null}
                <Link
                  to={`/tasks/${task.id}`}
                  className="rounded-lg px-3 py-1.5 text-xs text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200"
                >
                  查看
                </Link>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
