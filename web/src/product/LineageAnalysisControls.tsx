import { FormInput } from '../components/ui';
import ReferencePicker from './ReferencePicker';

export default function LineageAnalysisControls({
  payload,
  onChange,
  disabled,
  text,
}: {
  payload: Record<string, unknown>;
  onChange: (payload: Record<string, unknown>) => void;
  disabled: boolean;
  text: (zh: string, en: string) => string;
}) {
  const update = (key: string, value: unknown) => onChange({ ...payload, [key]: value });
  return (
    <div className="space-y-4">
      <ReferencePicker
        kind="metadata_object"
        label={text('分析起点', 'Starting object')}
        value={typeof payload.root_object_id === 'string' ? payload.root_object_id : ''}
        onChange={(value) => update('root_object_id', value)}
        disabled={disabled}
        required
      />
      <label className="block text-sm">
        <span className="mb-1.5 block">{text('分析方向', 'Direction')}</span>
        <select
          className="w-full rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-2"
          value={String(payload.direction ?? 'both')}
          disabled={disabled}
          onChange={(event) => update('direction', event.target.value)}
        >
          <option value="upstream">{text('上游追溯', 'Upstream')}</option>
          <option value="downstream">{text('下游影响', 'Downstream')}</option>
          <option value="both">{text('上游和下游', 'Upstream and downstream')}</option>
        </select>
      </label>
      <div className="grid gap-3 sm:grid-cols-3">
        {[
          ['max_depth', text('最大深度（1–20）', 'Depth (1–20)'), 5, 20],
          ['max_nodes', text('节点上限（1–500）', 'Node limit (1–500)'), 100, 500],
          ['max_edges', text('连线上限（1–1000）', 'Edge limit (1–1000)'), 200, 1000],
        ].map(([key, label, fallback, max]) => (
          <FormInput
            key={String(key)}
            label={String(label)}
            type="number"
            value={String(payload[String(key)] ?? fallback)}
            required
            disabled={disabled}
            min={1}
            max={Number(max)}
            step={1}
            onChange={(value) => update(String(key), value.trim() ? Number(value) : undefined)}
          />
        ))}
      </div>
      <p className="text-xs text-[var(--color-text-tertiary)]">
        {text(
          '分析使用已采集且版本有效的对象与血缘证据。达到节点、连线或深度上限时，结果会标明范围限制。',
          'Analysis uses collected objects with current versions and recorded lineage evidence. Results indicate any depth, node or edge limits.',
        )}
      </p>
    </div>
  );
}
