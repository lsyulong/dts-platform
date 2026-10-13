import { Link2, Tag, Unlink } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { useTagAssignments } from './api';
import type { ProductResource } from './resource-types';

export default function TagAssignmentsPanel({
  record,
  busy,
  canWrite,
  onUnassign,
}: {
  record: ProductResource;
  busy?: boolean;
  canWrite: boolean;
  onUnassign: (targetKind: string, targetId: string) => void;
}) {
  const { i18n } = useTranslation();
  const zh = i18n.language.startsWith('zh');
  const query = useTagAssignments(record.id);
  return (
    <section
      className="product-definition-field"
      aria-label={zh ? '当前标签绑定' : 'Current tag assignments'}
    >
      <h4>
        <Link2 size={15} aria-hidden="true" />
        {zh ? '当前标签绑定' : 'Current tag assignments'}
      </h4>
      {query.isPending ? (
        <p className="product-workspace-muted">
          {zh ? '正在读取绑定关系…' : 'Loading assignments…'}
        </p>
      ) : query.error ? (
        <p role="alert" className="product-workspace-muted">
          {zh ? '绑定关系读取失败' : 'Unable to load assignments'}
        </p>
      ) : !query.data?.data.length ? (
        <p className="product-workspace-muted">
          {zh ? '尚未绑定业务对象。' : 'No business objects are assigned.'}
        </p>
      ) : (
        <div className="grid gap-2">
          {query.data.data.map((item) => (
            <article
              key={item.id}
              className="rounded-lg border border-[var(--color-border)] bg-[var(--color-bg-primary)] p-3"
            >
              <div className="flex items-center justify-between gap-3">
                <div className="flex items-center gap-2">
                  <Tag size={14} aria-hidden="true" />
                  <strong>{item.target_name}</strong>
                </div>
                {canWrite && (
                  <button
                    type="button"
                    className="flex items-center gap-1 rounded px-2 py-1 text-xs font-medium text-[var(--color-error)] transition-colors hover:bg-[var(--color-error-subtle)] disabled:opacity-40"
                    disabled={busy}
                    onClick={() => onUnassign(item.target_kind, item.target_id)}
                  >
                    <Unlink size={13} aria-hidden="true" />
                    {zh ? '解绑' : 'Unassign'}
                  </button>
                )}
              </div>
              <p className="product-workspace-muted">
                <code>{item.target_kind}</code> · <code>{item.target_code}</code>
              </p>
              <p className="product-workspace-muted">
                {zh ? '绑定时间' : 'Assigned'}:{' '}
                {new Date(item.assigned_at).toLocaleString(i18n.language)}
              </p>
            </article>
          ))}
        </div>
      )}
    </section>
  );
}
