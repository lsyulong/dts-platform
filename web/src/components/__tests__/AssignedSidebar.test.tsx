import { beforeEach, describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import Sidebar from '../Sidebar';
import { setTestUser } from '../../test/test-utils';
import { useAppStore } from '../../stores/app';
import type {
  NavigationNode,
  NavigationPolicyState,
  NavigationView,
} from '../../product/navigation';

const node = (
  id: string,
  name: string,
  path: string,
  parent_id: string | null,
  sort_order: number,
): NavigationNode => ({
  id,
  name,
  path,
  parent_id,
  sort_order,
  menu_version: 1,
  icon: null,
  source: 'template',
  recovery: false,
});
function state(
  nodes: NavigationNode[],
  mode: NavigationView['mode'] = 'assigned',
): NavigationPolicyState {
  return {
    phase: 'ready',
    error: null,
    nodes,
    policy: {
      tenant_id: 'test-tenant',
      mode,
      assignment_id: 'assignment',
      assignment_version: 1,
      template_id: 'template',
      template_version: 1,
      expires_at: null,
      nodes,
      filtered_count: 0,
      observed_at: '2026-10-09T00:00:00Z',
      affects_navigation_only: true,
    },
    allowsPath: (path) => nodes.some((node) => node.path === path),
    refresh: async () => {},
  };
}
beforeEach(() => {
  setTestUser(['user:manage', 'task:read']);
  useAppStore.setState({ sidebarCollapsed: false, mobileSidebarOpen: false });
});
describe('real assigned sidebar policy', () => {
  it('uses frozen menu names and parent order and excludes unassigned catalog and aliases', () => {
    const policy = state([
      node('sibling', '第二菜单', '/query', null, 2),
      node('child', '运行记录', '/development/workflow-runs', 'parent', 0),
      node('parent', '第一菜单', '/tasks', null, 1),
    ]);
    render(
      <MemoryRouter>
        <Sidebar navigation={policy} />
      </MemoryRouter>,
    );
    expect(screen.getAllByRole('link').map((link) => link.textContent)).toEqual([
      '第一菜单',
      '运行记录',
      '第二菜单',
    ]);
    expect(screen.queryByRole('link', { name: 'nav.datasources' })).not.toBeInTheDocument();
    expect(screen.queryByRole('link', { name: 'nav.system' })).not.toBeInTheDocument();
    expect(screen.getByRole('link', { name: '运行记录' })).toHaveStyle({ paddingLeft: '26px' });
  });
  it('does not expose the full catalog while current policy is loading or failed', () => {
    const policy: NavigationPolicyState = { ...state([]), phase: 'loading', policy: null };
    const view = render(
      <MemoryRouter>
        <Sidebar navigation={policy} />
      </MemoryRouter>,
    );
    expect(screen.queryAllByRole('link')).toHaveLength(0);
    view.rerender(
      <MemoryRouter>
        <Sidebar navigation={{ ...policy, phase: 'error', error: new Error('policy denied') }} />
      </MemoryRouter>,
    );
    expect(screen.queryAllByRole('link')).toHaveLength(0);
  });
});
