import { useTranslation } from 'react-i18next';
import { Button } from '../../components/ui/button';
import { ErrorBanner } from '../../components/ui';
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '../../components/ui/dialog';
import ExpiryInput from '../ExpiryInput';
import type { useNavigationOperations } from './operations';

export default function NavigationOperationDialog({
  operations,
}: {
  operations: ReturnType<typeof useNavigationOperations>;
}) {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const { pending, setPending, busy, error, close, commit } = operations;
  return (
    <Dialog
      open={!!pending}
      onOpenChange={(open) => {
        if (!open) close();
      }}
    >
      <DialogContent>
        <DialogTitle>
          {pending?.kind === 'assign'
            ? text('分配导航模板', 'Assign navigation template')
            : text('撤销导航策略', 'Revoke navigation policy')}
        </DialogTitle>
        <DialogDescription>
          {pending?.kind === 'assign'
            ? `${pending.templateName} · v${pending.templateVersion}`
            : text(
                '撤销后恢复当前角色有权访问的默认目录',
                'Revocation restores the default catalog allowed by current roles',
              )}
        </DialogDescription>
        <form
          className="mt-4 space-y-4"
          onSubmit={(event) => {
            event.preventDefault();
            void commit();
          }}
        >
          <p className="text-sm">
            {text('当前策略版本', 'Current assignment version')}: {pending?.assignmentVersion}
          </p>
          {pending?.kind === 'assign' && (
            <>
              <p className="text-sm text-[var(--color-text-secondary)]">
                {text(
                  '将冻结这次模板与菜单版本；修改定义后需要重新分配。此操作只影响导航，不授予 API 权限。',
                  'This freezes the template and menu versions. Definition changes require reassignment. Navigation does not grant API permissions.',
                )}
              </p>
              <ExpiryInput
                label={text('策略到期时间', 'Policy expiry')}
                value={pending.expiresAt}
                disabled={busy}
                onChange={(expiresAt) =>
                  setPending((current) =>
                    current?.kind === 'assign' ? { ...current, expiresAt } : current,
                  )
                }
                description={
                  pending.validityDays === null
                    ? text('留空表示不设置到期时间', 'Leave empty for no expiry')
                    : text(
                        `留空使用服务器计算的 ${pending.validityDays} 天模板有效期；指定时间不能超过该期限`,
                        `Leave empty for the server-calculated ${pending.validityDays}-day template validity; an explicit date cannot extend it`,
                      )
                }
              />
            </>
          )}
          {pending?.kind === 'revoke' && (
            <div>
              <label htmlFor="navigation-revoke-reason" className="mb-1.5 block text-sm">
                {text('撤销原因', 'Revocation reason')}
              </label>
              <textarea
                id="navigation-revoke-reason"
                required
                maxLength={8192}
                value={pending.reason}
                disabled={busy}
                onChange={(event) =>
                  setPending((current) =>
                    current?.kind === 'revoke'
                      ? { ...current, reason: event.target.value }
                      : current,
                  )
                }
                className="w-full rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-3 text-sm"
              />
            </div>
          )}
          {error && <ErrorBanner message={error} />}
          <div className="flex justify-end gap-2">
            <Button type="button" variant="outline" onClick={close}>
              {text('关闭', 'Close')}
            </Button>
            <Button type="submit" disabled={busy}>
              {busy ? text('正在提交…', 'Submitting…') : text('确认', 'Confirm')}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}
