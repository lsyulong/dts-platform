import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import TagAssignmentsPanel from '../TagAssignmentsPanel';
import { initialActionPayload } from '../action-models';
import type { ProductResource } from '../resource-types';

const api = vi.hoisted(() => ({ assignments: vi.fn() }));
vi.mock('../api', () => ({ useTagAssignments: api.assignments }));

const tag: ProductResource = {
  id: 'tag-one',
  tenant_id: 'tenant-a',
  kind: 'tag',
  code: 'customer',
  name: '客户',
  description: null,
  parent_id: null,
  status: 'enabled',
  properties: { color: '#16a34a', scope: 'assets' },
  references: [],
  version: 3,
  created_by: 'actor',
  created_at: '2026-10-09T00:00:00Z',
  updated_at: '2026-10-09T00:00:00Z',
};

describe('tag assignments', () => {
  beforeEach(() => {
    api.assignments.mockReturnValue({
      isPending: false,
      error: null,
      data: {
        total: 1,
        data: [
          {
            id: 'assignment-one',
            tenant_id: 'tenant-a',
            tag_id: tag.id,
            tag_version: 3,
            target_kind: 'asset_entry',
            target_id: 'asset-one',
            target_code: 'customer-orders',
            target_name: '客户订单',
            assigned_by: 'actor',
            assigned_at: '2026-10-09T01:00:00Z',
          },
        ],
      },
    });
  });

  it('shows durable relationships and starts unassign with the exact target', async () => {
    const user = userEvent.setup();
    const onUnassign = vi.fn();
    render(<TagAssignmentsPanel record={tag} canWrite onUnassign={onUnassign} />);
    expect(screen.getByText('客户订单')).toBeInTheDocument();
    expect(screen.getByText(/customer-orders/)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: /解绑|Unassign/ }));
    expect(onUnassign).toHaveBeenCalledWith('asset_entry', 'asset-one');
  });

  it('does not offer a write action to read-only users', () => {
    render(<TagAssignmentsPanel record={tag} canWrite={false} onUnassign={vi.fn()} />);
    expect(screen.queryByRole('button', { name: /解绑|Unassign/ })).not.toBeInTheDocument();
  });

  it('prepares a fresh idempotency key for both relationship mutations', () => {
    for (const action of ['assign', 'unassign']) {
      const payload = initialActionPayload(tag, action);
      expect(payload).toMatchObject({ target_kind: '', target_id: '' });
      expect(payload.idempotency_key).toEqual(expect.any(String));
    }
  });
});
