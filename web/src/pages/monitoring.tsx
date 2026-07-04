import { useEffect, useState } from 'react';
import { Activity, Zap, Globe, Server } from 'lucide-react';
import MetricCard from '../components/MetricCard';
import LogConsole from '../components/LogConsole';
import client from '../api/client';

interface AlertRule {
  id: string;
  name: string;
  metric_type: string;
  operator: string;
  threshold: number;
  enabled: boolean;
  task_name: string | null;
  created_at: string;
}

export default function MonitoringPage() {
  const [alerts, setAlerts] = useState<AlertRule[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    client
      .get('/alerts')
      .then((res) => setAlerts(res.data.data || []))
      .catch(console.error)
      .finally(() => setLoading(false));
  }, []);

  const toggleAlert = async (id: string) => {
    await client.patch(`/alerts/${id}/toggle`);
    setAlerts((prev) =>
      prev.map((a) => (a.id === id ? { ...a, enabled: !a.enabled } : a))
    );
  };

  const metricLabel: Record<string, string> = {
    latency: '延迟 (ms)',
    error_rate: '错误率 (%)',
    throughput: '吞吐量 (MB/s)',
    row_count: '行数',
  };

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-xl font-bold text-zinc-100">监控告警</h1>
        <p className="mt-1 text-sm text-zinc-500">系统运行状态监控与告警配置</p>
      </div>

      {/* System health (mock) */}
      <div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
        <MetricCard title="CPU 使用率" value="42" unit="%" subtitle="正常范围" trend="neutral" icon={<Server size={18} />} />
        <MetricCard title="内存" value="18.4" unit="GB" subtitle="共 32 GB" icon={<Activity size={18} />} />
        <MetricCard title="网络 I/O" value="845" unit="MB/s" subtitle="↑ 620 ↓ 225" icon={<Globe size={18} />} />
        <MetricCard title="平均延迟" value="45" unit="ms" subtitle="P99: 120ms" trend="up" icon={<Zap size={18} />} />
      </div>

      {/* Alert rules */}
      <div className="rounded-2xl border border-zinc-800/60 bg-zinc-900/40 p-5 backdrop-blur-sm">
        <h3 className="mb-4 text-sm font-medium text-zinc-300">告警规则</h3>
        {loading ? (
          <div className="flex justify-center py-6">
            <div className="h-6 w-6 animate-spin rounded-full border-2 border-zinc-600 border-t-blue-400" />
          </div>
        ) : alerts.length === 0 ? (
          <p className="text-sm text-zinc-600">暂无告警规则</p>
        ) : (
          <div className="space-y-2">
            {alerts.map((alert) => (
              <div
                key={alert.id}
                className="flex items-center justify-between rounded-xl border border-zinc-800/40 bg-zinc-900/20 p-3"
              >
                <div>
                  <p className="text-sm font-medium text-zinc-200">{alert.name}</p>
                  <p className="text-xs text-zinc-500">
                    {metricLabel[alert.metric_type] || alert.metric_type} {alert.operator} {alert.threshold}
                  </p>
                </div>
                <button
                  onClick={() => toggleAlert(alert.id)}
                  className={`relative h-6 w-11 rounded-full transition-colors ${
                    alert.enabled ? 'bg-blue-500' : 'bg-zinc-700'
                  }`}
                >
                  <div
                    className={`absolute top-0.5 h-5 w-5 rounded-full bg-white shadow transition-transform ${
                      alert.enabled ? 'translate-x-5.5' : 'translate-x-0.5'
                    }`}
                    style={{ left: alert.enabled ? undefined : 0 }}
                  />
                </button>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Log console */}
      <LogConsole
        title="全局日志"
        logs={[
          { timestamp: '10:30:45', level: 'INFO', component: 'monitor', message: '系统健康检查通过' },
          { timestamp: '10:30:30', level: 'INFO', component: 'task-mysql2pg', message: '数据同步进度: 45.2%' },
          { timestamp: '10:30:15', level: 'WARN', component: 'datasource-prod-db', message: '连接池使用率 > 80%' },
        ]}
      />
    </div>
  );
}
