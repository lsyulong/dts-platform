import { useCallback, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '../components/ui/button';
import { ErrorBanner, FormInput } from '../components/ui';
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '../components/ui/dialog';
import type { ProductResource } from './resource-types';
import { productText, statusText } from './ui-text';
import ReferencePicker from './ReferencePicker';
import ActionParameters from './ActionParameters';
import RuntimeActionParameters from './RuntimeActionParameters';
import MetricRunControls from './MetricRunControls';
import ExpiryInput from './ExpiryInput';
import LineageAnalysisControls from './LineageAnalysisControls';
import MaskingPreviewControls from './MaskingPreviewControls';
import IdMultiSelectEditor from './business-editors/IdMultiSelectEditor';
import type { PendingResourceAction } from './action-models';
import { productModules } from './catalog';
import { localized } from './ui-text';

export default function ResourceOperationDialog({
  deletion,
  action,
  busy,
  error,
  onChange,
  onClose,
  onCommit,
}: {
  deletion: ProductResource | null;
  action: PendingResourceAction | null;
  busy: boolean;
  error: string;
  onChange: (payload: Record<string, unknown>) => void;
  onClose: () => void;
  onCommit: () => void;
}) {
  const { i18n } = useTranslation();
  const copy = productText(i18n.language);
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const payload = action?.payload ?? {};
  const [runtimeAvailability, setRuntimeAvailability] = useState<{
    key: string;
    ready: boolean;
  } | null>(null);
  const onRuntimeAvailability = useCallback((key: string, ready: boolean) => {
    setRuntimeAvailability((current) =>
      current?.key === key && current.ready === ready ? current : { key, ready },
    );
  }, []);
  const runtimeKey =
    action &&
    ['execute', 'preview'].includes(action.name) &&
    ['sql_document', 'development_workflow'].includes(action.record.kind) &&
    action.record.properties.configuration_id
      ? JSON.stringify([
          action.record.kind,
          action.record.id,
          action.record.version,
          action.record.properties.configuration_id,
        ])
      : undefined;
  const runtimeBlocked =
    !!runtimeKey && (runtimeAvailability?.key !== runtimeKey || !runtimeAvailability.ready);
  const parameters =
    payload.parameters &&
    typeof payload.parameters === 'object' &&
    !Array.isArray(payload.parameters)
      ? (payload.parameters as Record<string, unknown>)
      : {};
  const update = (key: string, value: unknown) => onChange({ ...payload, [key]: value });
  return (
    <Dialog
      open={!!deletion || !!action}
      onOpenChange={(open) => {
        if (!open && !busy) onClose();
      }}
    >
      <DialogContent>
        <DialogTitle>
          {deletion ? copy.removeTitle : statusText(action?.name ?? '', i18n.language)}
        </DialogTitle>
        <DialogDescription>{deletion ? copy.removeConfirm : action?.record.name}</DialogDescription>
        <form
          className="mt-4 space-y-4"
          onSubmit={(event) => {
            event.preventDefault();
            if (!runtimeBlocked) onCommit();
          }}
        >
          {action?.record.kind === 'metadata_lineage' && action.name === 'analyze' && (
            <LineageAnalysisControls
              payload={payload}
              onChange={onChange}
              disabled={busy}
              text={text}
            />
          )}
          {action?.record.kind === 'asset_map' && action.name === 'analyze' && (
            <div className="grid gap-4 sm:grid-cols-2">
              {(['max_nodes', 'max_edges'] as const).map((key) => (
                <FormInput
                  key={key}
                  label={
                    key === 'max_nodes'
                      ? text('节点上限', 'Node limit')
                      : text('关系上限', 'Edge limit')
                  }
                  type="number"
                  min={1}
                  max={key === 'max_nodes' ? 500 : 2000}
                  step={1}
                  required
                  value={typeof payload[key] === 'number' ? payload[key] : ''}
                  onChange={(value) => update(key, value === '' ? undefined : Number(value))}
                  disabled={busy}
                />
              ))}
            </div>
          )}
          {action?.record.kind === 'masking_rule' && action.name === 'preview' && (
            <MaskingPreviewControls
              value={payload.sample}
              onChange={(value) => update('sample', value)}
              disabled={busy}
              text={text}
            />
          )}
          {action?.record.kind === 'user_group' && action.name === 'assign_members' && (
            <IdMultiSelectEditor
              kind="user_group"
              fieldKey="member_ids"
              value={payload.member_ids}
              onChange={(value) => update('member_ids', value)}
              disabled={busy}
            />
          )}
          {action?.record.kind === 'tag' && ['assign', 'unassign'].includes(action.name) && (
            <>
              <label className="grid gap-1.5 text-sm font-medium">
                {text('目标业务类型', 'Target resource type')}
                <select
                  className="h-10 rounded-md border border-[var(--color-border)] bg-[var(--color-bg-primary)] px-3 text-sm"
                  required
                  value={typeof payload.target_kind === 'string' ? payload.target_kind : ''}
                  onChange={(event) =>
                    onChange({ ...payload, target_kind: event.target.value, target_id: '' })
                  }
                  disabled={busy}
                >
                  <option value="">{text('请选择', 'Select')}</option>
                  {productModules
                    .filter(
                      (module) =>
                        module.resourceKind &&
                        module.resourceKind !== 'tag' &&
                        (action.record.properties.scope === 'all' ||
                          module.groupId === action.record.properties.scope),
                    )
                    .map((module) => (
                      <option key={module.resourceKind} value={module.resourceKind}>
                        {localized(module.title, i18n.language)}
                      </option>
                    ))}
                </select>
              </label>
              {typeof payload.target_kind === 'string' && payload.target_kind && (
                <ReferencePicker
                  kind={payload.target_kind}
                  label={text('目标业务对象', 'Target resource')}
                  value={typeof payload.target_id === 'string' ? payload.target_id : ''}
                  onChange={(value) => update('target_id', value)}
                  disabled={busy}
                  required
                />
              )}
              <p className="text-sm text-[var(--color-text-tertiary)]">
                {action.name === 'assign'
                  ? text(
                      '绑定会写入独立关系记录；目标或标签在解绑前不能删除。',
                      'The relationship is stored separately; neither record can be deleted until it is unassigned.',
                    )
                  : text(
                      '解绑会删除当前关系并保留不可变操作回执。',
                      'Unassigning removes the active relationship and keeps an immutable operation receipt.',
                    )}
              </p>
            </>
          )}
          {action &&
            action.record.kind !== 'asset_entry' &&
            ([
              'approve',
              'reject',
              'withdraw',
              'revoke',
              'register_schedule',
              'unschedule',
            ].includes(action.name) ||
              (action.record.kind === 'development_config' && action.name === 'publish')) && (
              <FormInput
                label={copy.comment}
                type="textarea"
                value={
                  typeof payload.reason === 'string'
                    ? payload.reason
                    : typeof payload.comment === 'string'
                      ? payload.comment
                      : ''
                }
                required={
                  ['approve', 'reject', 'revoke', 'register_schedule', 'unschedule'].includes(
                    action.name,
                  ) || action.record.kind === 'development_config'
                }
                onChange={(comment) =>
                  onChange(
                    [
                      'data_access_request',
                      'service_subscription',
                      'development_config',
                      'development_workflow',
                    ].includes(action.record.kind)
                      ? { ...payload, reason: comment }
                      : { ...payload, comment, reason: comment },
                  )
                }
                disabled={busy}
              />
            )}
          {action?.name === 'validate' && action.record.kind === 'naming_standard' && (
            <FormInput
              label={copy.sample}
              value={typeof payload.sample === 'string' ? payload.sample : ''}
              onChange={(value) => update('sample', value)}
              disabled={busy}
            />
          )}
          {action &&
            ['execute', 'preview'].includes(action.name) &&
            (['sql_document', 'development_workflow'].includes(action.record.kind) ? (
              <RuntimeActionParameters
                record={action.record}
                value={parameters}
                onChange={(value) => update('parameters', value)}
                disabled={busy}
                text={text}
                availabilityKey={runtimeKey}
                onAvailability={onRuntimeAvailability}
              />
            ) : (
              <ActionParameters
                record={action.record}
                value={parameters}
                onChange={(value) => update('parameters', value)}
                disabled={busy}
                text={text}
              />
            ))}
          {action &&
            action.name === 'calculate' &&
            ['atomic_metric', 'derived_metric', 'composite_metric'].includes(
              action.record.kind,
            ) && (
              <MetricRunControls
                record={action.record}
                payload={payload}
                onChange={onChange}
                disabled={busy}
                text={text}
              />
            )}
          {action &&
            ['data_access_request', 'service_subscription'].includes(action.record.kind) &&
            action.name === 'approve' && (
              <ExpiryInput
                label={text('授权失效时间（可缩短）', 'Grant expiry (may be shortened)')}
                value={payload.expires_at}
                onChange={(value) => update('expires_at', value)}
                disabled={busy}
                maximum={
                  typeof action.record.properties.expires_at === 'string'
                    ? action.record.properties.expires_at
                    : undefined
                }
                description={text(
                  '留空沿用申请时间；审批不能延长申请的有效期',
                  'Leave empty to use the requested expiry; approval cannot extend it',
                )}
              />
            )}
          {action?.name === 'convert' && (
            <>
              <FormInput
                label={text('待换算数值', 'Value to convert')}
                type="number"
                required
                value={typeof payload.value === 'number' ? String(payload.value) : ''}
                onChange={(value) => update('value', value.trim() ? Number(value) : undefined)}
                disabled={busy}
              />
              <ReferencePicker
                kind="unit_standard"
                label={text('目标计量单位', 'Target unit')}
                value={typeof payload.target_id === 'string' ? payload.target_id : ''}
                onChange={(value) => update('target_id', value)}
                disabled={busy}
                required
              />
            </>
          )}
          {action?.name === 'send' && (
            <p className="text-sm text-[var(--color-text-tertiary)]">
              {text(
                '发送后保存每位收件人的实际投递记录。请求失败时可在此重试，不会重复发送已经提交的批次。',
                'Delivery records are saved for each recipient. Retrying this request preserves its idempotency key.',
              )}
            </p>
          )}
          {action?.record.kind === 'alert_policy' &&
            ['evaluate', 'test_delivery'].includes(action.name) && (
              <p className="text-sm text-[var(--color-text-tertiary)]">
                {action.name === 'evaluate'
                  ? text(
                      '服务端会统计真实任务、质量或健康记录；达到阈值时生成发件箱记录，冷却期内只记录抑制结果。',
                      'The server counts persisted task, quality, or health records. Threshold matches create outbox entries; cooldown matches are recorded as suppressed.',
                    )
                  : text(
                      '测试投递会生成带测试标识的真实发件箱记录。未配置投递驱动时状态保持待投递，不会显示为已发送。',
                      'A test creates real test outbox entries. Without a delivery driver they remain pending and are never shown as sent.',
                    )}
              </p>
            )}
          {error && <ErrorBanner message={error} />}
          <div className="flex justify-end gap-2">
            <Button type="button" variant="outline" disabled={busy} onClick={onClose}>
              {copy.cancel}
            </Button>
            <Button
              type="submit"
              disabled={
                busy ||
                runtimeBlocked ||
                (action?.name === 'convert' &&
                  (!payload.target_id ||
                    typeof payload.value !== 'number' ||
                    !Number.isFinite(payload.value))) ||
                (action?.record.kind === 'tag' &&
                  ['assign', 'unassign'].includes(action.name) &&
                  (!payload.target_kind || !payload.target_id))
              }
            >
              {busy ? copy.loading : deletion ? copy.remove : copy.save}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}
