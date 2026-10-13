import { BellRing, CircleCheck, Clock3, FlaskConical, ShieldAlert } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { useAlertPolicyEvaluations } from './api';
import type { ProductResource } from './resource-types';

const stateIcon = {
  triggered: ShieldAlert,
  within_threshold: CircleCheck,
  suppressed: Clock3,
  test: FlaskConical,
};

export default function AlertPolicyEvaluationsPanel({ record }: { record: ProductResource }) {
  const { i18n } = useTranslation();
  const zh = i18n.language.startsWith('zh');
  const query = useAlertPolicyEvaluations(record.id);
  return (
    <section
      className="product-definition-field"
      aria-label={zh ? '策略评估记录' : 'Policy evaluations'}
    >
      <h4>
        <BellRing size={15} aria-hidden="true" />
        {zh ? '策略评估记录' : 'Policy evaluations'}
      </h4>
      {query.isPending ? (
        <p className="product-workspace-muted">
          {zh ? '正在读取评估记录…' : 'Loading evaluations…'}
        </p>
      ) : query.error ? (
        <p role="alert" className="product-workspace-muted">
          {zh ? '评估记录读取失败' : 'Unable to load evaluations'}
        </p>
      ) : !query.data?.data.length ? (
        <p className="product-workspace-muted">
          {zh ? '尚未执行策略评估。' : 'No policy evaluation has run.'}
        </p>
      ) : (
        <div className="grid gap-2">
          {query.data.data.map((item) => {
            const Icon = stateIcon[item.state];
            return (
              <article
                key={item.id}
                className="rounded-lg border border-[var(--color-border)] bg-[var(--color-bg-primary)] p-3"
              >
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <strong className="flex items-center gap-2">
                    <Icon size={14} aria-hidden="true" />
                    {item.state}
                  </strong>
                  <time>{new Date(item.created_at).toLocaleString(i18n.language)}</time>
                </div>
                <p className="product-workspace-muted">
                  {zh ? '真实事件计数' : 'Observed events'}: {item.observed_count} /{' '}
                  {zh ? '阈值' : 'threshold'} {item.threshold}
                </p>
                <p className="product-workspace-muted">
                  {zh ? '投递发件箱' : 'Delivery outbox'}:{' '}
                  {item.deliveries.length
                    ? item.deliveries
                        .map(
                          (delivery) =>
                            `${delivery.recipient_id} · ${delivery.channel} · ${delivery.status} · ${zh ? '尝试' : 'attempts'} ${delivery.attempt_count}${delivery.last_error ? ` · ${delivery.last_error}` : ''}`,
                        )
                        .join('；')
                    : zh
                      ? '未生成'
                      : 'none'}
                </p>
                {item.state === 'suppressed' && item.cooldown_until && (
                  <p className="product-workspace-muted">
                    {zh ? '抑制至' : 'Suppressed until'}:{' '}
                    {new Date(item.cooldown_until).toLocaleString(i18n.language)}
                  </p>
                )}
              </article>
            );
          })}
        </div>
      )}
    </section>
  );
}
