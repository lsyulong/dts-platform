import { useTranslation } from 'react-i18next';
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '../../components/ui/dialog';
import { Button } from '../../components/ui/button';
import { ErrorBanner } from '../../components/ui';
import type { RevisionOperation } from './operations';
export default function RevisionConfirm({
  pending,
  busy,
  error,
  onChange,
  onClose,
  onCommit,
}: {
  pending: RevisionOperation | null;
  busy: boolean;
  error: string;
  onChange: (value: RevisionOperation) => void;
  onClose: () => void;
  onCommit: () => void;
}) {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  return (
    <Dialog
      open={!!pending}
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <DialogContent>
        <DialogTitle>
          {pending?.action === 'restore'
            ? text('确认恢复历史修订', 'Confirm revision restore')
            : text('保存当前版本修订', 'Save current version revision')}
        </DialogTitle>
        <DialogDescription>
          {pending?.action === 'restore'
            ? text(
                '恢复会生成新草稿，覆盖当前可编辑定义。需另行发布和执行，不影响已有运行快照。',
                'Restoring creates a new draft and replaces the editable definition. Publish and execute separately; existing run snapshots remain.',
              )
            : text(
                '冻结数据库中的当前完整定义和实际版本，不发布或执行查询。',
                'Freeze the complete current definition and actual version from storage. This does not publish or execute queries.',
              )}
        </DialogDescription>
        {pending && (
          <div className="space-y-4">
            <p>
              {text('当前校验版本', 'Expected current version')}: v{pending.version}
            </p>
            {pending.revisionId && (
              <p className="break-all">
                {text('历史修订', 'Revision')}: {pending.revisionId}
              </p>
            )}
            <label className="block">
              {text('修订原因', 'Revision reason')}
              <textarea
                aria-label={text('修订原因', 'Revision reason')}
                value={pending.reason}
                disabled={busy}
                onChange={(event) => onChange({ ...pending, reason: event.target.value })}
                className="mt-2 w-full rounded border border-[var(--color-border-default)] p-2"
              />
            </label>
            {error && <ErrorBanner message={error} />}
            <div className="flex justify-end gap-2">
              <Button variant="outline" onClick={onClose}>
                {text('关闭', 'Close')}
              </Button>
              <Button disabled={busy || !pending.reason.trim()} onClick={onCommit}>
                {busy ? text('提交中…', 'Submitting…') : text('确认提交', 'Confirm')}
              </Button>
            </div>
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}
