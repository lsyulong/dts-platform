export interface QualityObservation {
  id: string;
  monitor_id: string;
  monitor_version: number;
  object_id: string;
  actor_id: string;
  period_start: string;
  period_end: string;
  state: 'threshold_exceeded' | 'within_threshold' | 'unavailable';
  reason: string;
  summary: {
    completed_runs: number | string;
    failed_runs: number | string;
    incomplete_runs: number | string;
    checked_rows: number | string;
    failed_rows: number | string;
    failure_ratio: number | null;
    threshold: number | null;
    threshold_exceeded: boolean | null;
    window_complete: boolean;
    sampled_checks: number | string;
    counting_unit: string;
  };
  result_access: string;
  dependencies: { kind: string; id: string; version: number }[];
  run_ids: string[];
  created_at: string;
}
export interface QualityAlert {
  id: string;
  tenant_id: string;
  monitor_id: string;
  monitor_version: number;
  status: 'open' | 'acknowledged' | 'resolved';
  version: number;
  opening_observation_id: string;
  last_observation_id: string;
  acknowledged_by: string | null;
  acknowledged_at: string | null;
  resolved_at: string | null;
  created_at: string;
  updated_at: string;
}
export interface QualityAlertView {
  alert: QualityAlert;
  observation: QualityObservation;
}
export interface QualityEvaluation {
  observation: QualityObservation;
  alert: QualityAlert | null;
  replayed: boolean;
}
export interface QualityAckRequest {
  expected_version: number;
  idempotency_key: string;
  reason: string;
}
export interface QualityNotice {
  id: string;
  alert_id: string;
  recipient_id: string;
  subject: string;
  link: string;
  delivery_status: 'pending' | 'unconfigured' | 'failed' | 'delivered';
  read_at: string | null;
  created_at: string;
}
export interface QualityEvent {
  id: string;
  observation_id: string;
  actor_id: string;
  action: string;
  reason: string;
  before_status: string | null;
  after_status: string;
  created_at: string;
}
export const qualityReasons: Record<string, [string, string]> = {
  policy_changed: [
    '监控策略版本已变更，不能以新阈值假装旧告警恢复',
    'Monitor policy changed; the new threshold does not establish recovery of the old incident',
  ],
  threshold_exceeded: ['失败比例超过阈值', 'Failure ratio exceeds the threshold'],
  within_threshold: ['实际计数在阈值内', 'Observed counts are within the threshold'],
  sample_within_threshold: ['采样计数在阈值内', 'Sampled counts are within the threshold'],
  execution_error: ['执行失败，无法评估', 'Execution failed; evaluation unavailable'],
  incomplete_window: ['运行窗口不完整', 'The run window is incomplete'],
  incomplete_runs: ['窗口内有尚未完成的运行', 'Some runs in the window are incomplete'],
  no_results: ['窗口内暂无真实运行结果', 'No actual run results in this window'],
  empty_data: ['没有实际数据，无法评估', 'No observed data; evaluation unavailable'],
  partial_empty_data: [
    '部分规则没有数据，无法评估',
    'Some checks have no data; evaluation unavailable',
  ],
};
