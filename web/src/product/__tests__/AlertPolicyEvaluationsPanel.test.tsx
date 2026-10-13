import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import AlertPolicyEvaluationsPanel from '../AlertPolicyEvaluationsPanel';
import { initialActionPayload } from '../action-models';
import type { ProductResource } from '../resource-types';

const api = vi.hoisted(() => ({ evaluations: vi.fn() }));
vi.mock('../api', () => ({ useAlertPolicyEvaluations: api.evaluations }));

const policy: ProductResource = {
  id: 'policy',
  tenant_id: 'tenant',
  kind: 'alert_policy',
  code: 'failures',
  name: '失败告警',
  description: null,
  parent_id: null,
  status: 'draft',
  properties: {
    event_type: 'task_failure',
    enabled: true,
    window_seconds: 3600,
    threshold: 1,
    recipient_ids: ['user'],
    cooldown_seconds: 600,
  },
  references: [],
  version: 1,
  created_by: 'actor',
  created_at: '2026-10-10T00:00:00Z',
  updated_at: '2026-10-10T00:00:00Z',
};

describe('alert policy evaluations', () => {
  it('shows persisted event counts, cooldown state, and honest outbox status', () => {
    api.evaluations.mockReturnValue({
      isPending: false,
      error: null,
      data: {
        total: 1,
        data: [
          {
            id: 'evaluation',
            policy_id: 'policy',
            policy_version: 1,
            event_type: 'task_failure',
            window_start: '2026-10-10T00:00:00Z',
            window_end: '2026-10-10T01:00:00Z',
            observed_count: 3,
            threshold: 1,
            state: 'triggered',
            cooldown_until: '2026-10-10T01:10:00Z',
            actor_id: 'actor',
            created_at: '2026-10-10T01:00:00Z',
            replayed: false,
            deliveries: [
              {
                id: 'delivery',
                recipient_id: 'user',
                subject: '告警',
                body: '失败',
                channel: 'in_app',
                status: 'pending',
                attempt_count: 0,
                last_error: null,
                next_attempt_at: '2026-10-10T01:00:00Z',
                delivered_at: null,
                created_at: '2026-10-10T01:00:00Z',
              },
            ],
          },
        ],
      },
    });
    render(<AlertPolicyEvaluationsPanel record={policy} />);
    expect(screen.getByText(/3 \/ .*1/)).toBeInTheDocument();
    expect(screen.getByText(/user · in_app · pending · 尝试 0/)).toBeInTheDocument();
  });
  it('gives evaluate and test delivery independent idempotency keys', () => {
    const evaluate = initialActionPayload(policy, 'evaluate');
    const test = initialActionPayload(policy, 'test_delivery');
    expect(evaluate.idempotency_key).toEqual(expect.any(String));
    expect(test.idempotency_key).toEqual(expect.any(String));
    expect(evaluate.idempotency_key).not.toBe(test.idempotency_key);
  });
});
