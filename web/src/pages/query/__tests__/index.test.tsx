import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act, screen, fireEvent } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { renderWithProviders } from '../../../test/test-utils';
import QueryPage from '../index';
import { useAuthStore } from '../../../stores/auth';
import { queryHistoryKey } from '../../../lib/session';
import { useAppStore } from '../../../stores/app';

const mockPost = vi.fn();
const mockGet = vi.fn();
vi.mock('../../../api/client', () => ({
  default: {
    post: (...args: unknown[]) => mockPost(...args),
    get: (...args: unknown[]) => mockGet(...args),
  },
}));
vi.mock('@monaco-editor/react', () => ({
  default: ({
    value,
    onChange,
    theme,
  }: {
    value: string;
    onChange: (value: string) => void;
    theme: string;
  }) => (
    <textarea
      aria-label="SQL"
      data-theme={theme}
      value={value}
      onChange={(event) => onChange(event.target.value)}
    />
  ),
}));

describe('QueryPage', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    localStorage.clear();
    useAuthStore.setState({ user: null, token: null });
  });

  it('follows the selected editor theme without resetting the query', () => {
    useAppStore.setState({ theme: 'light' });
    renderWithProviders(<QueryPage />);
    expect(screen.getByLabelText('SQL')).toHaveAttribute('data-theme', 'vs');
    act(() => useAppStore.setState({ theme: 'dark' }));
    expect(screen.getByLabelText('SQL')).toHaveAttribute('data-theme', 'vs-dark');
    expect(screen.getByLabelText('SQL')).toHaveValue('SELECT 1 AS hello');
  });

  it('discards pending results when the login session changes', async () => {
    let resolve: (value: unknown) => void = () => {};
    mockPost.mockReturnValue(new Promise((done) => { resolve = done; }));
    const user = userEvent.setup();
    renderWithProviders(<QueryPage />);
    await user.click(screen.getByRole('button', { name: 'query.execute' }));
    const request = mockPost.mock.calls[0][2];
    act(() => useAuthStore.setState({ token: 'replacement-session' }));
    expect(request.signal.aborted).toBe(true);
    await act(async () => resolve({ data: {
      columns: [{ name: 'value', data_type: 'VARCHAR', is_numeric: false }],
      rows: [{ value: 'old-session-secret' }], row_count: 1, suggested_chart: null,
    } }));
    expect(screen.queryByText('old-session-secret')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'query.execute' })).toBeEnabled();
  });

  it('removes the old catalog when a new authorization request fails', async () => {
    mockGet.mockResolvedValueOnce({ data: [{ catalog: 'lake', schema: 'main', tables: [{ name: 'private_orders' }] }] })
      .mockRejectedValueOnce({ response: { data: { message: 'Catalog permission revoked' } } });
    const user = userEvent.setup();
    renderWithProviders(<QueryPage />);
    const button = screen.getByRole('button', { name: 'query.schemas' });
    await user.click(button);
    expect(await screen.findByRole('button', { name: 'private_orders' })).toBeInTheDocument();
    await user.click(button);
    await user.click(button);
    expect(await screen.findByText('Catalog permission revoked')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'private_orders' })).not.toBeInTheDocument();
  });

  it('shows exact decimal and integer strings and readable structured values', async () => {
    mockPost.mockResolvedValue({
      data: {
        columns: [
          { name: 'amount', data_type: 'Decimal', is_numeric: true },
          { name: 'id', data_type: 'UInt64', is_numeric: true },
          { name: 'detail', data_type: 'Struct', is_numeric: false },
        ],
        rows: [
          {
            amount: '1234567890.123456789',
            id: '18446744073709551615',
            detail: { tags: ['one', 'two'] },
          },
        ],
        row_count: 1,
        suggested_chart: null,
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<QueryPage />);
    await user.click(screen.getByRole('button', { name: 'query.execute' }));
    expect(await screen.findByText('1234567890.123456789')).toBeInTheDocument();
    expect(screen.getByText('18446744073709551615')).toBeInTheDocument();
    expect(screen.getByText('{"tags":["one","two"]}')).toBeInTheDocument();
    expect(mockPost).toHaveBeenCalledWith(
      '/query/execute',
      { sql: 'SELECT 1 AS hello' },
      expect.objectContaining({ timeout: 65000, signal: expect.any(AbortSignal) }),
    );
  });

  it('distinguishes matching schema names across catalogs and quotes identifiers', async () => {
    mockGet.mockResolvedValue({
      data: [
        { catalog: 'warehouse', schema: 'main', tables: [{ name: 'customers' }] },
        { catalog: 'archive', schema: 'main', tables: [{ name: 'customer"history' }] },
      ],
    });
    const user = userEvent.setup();
    renderWithProviders(<QueryPage />);
    await user.click(screen.getByRole('button', { name: 'query.schemas' }));
    expect(await screen.findByText('warehouse.main')).toBeInTheDocument();
    expect(screen.getByText('archive.main')).toBeInTheDocument();
    await user.clear(screen.getByLabelText('SQL'));
    await user.type(screen.getByLabelText('SQL'), 'SELECT * FROM ');
    await user.click(screen.getByRole('button', { name: 'customer"history' }));
    expect(screen.getByLabelText('SQL')).toHaveValue(
      'SELECT * FROM "archive"."main"."customer""history"',
    );
  });

  it('preserves the server error with its actionable LIMIT hint', async () => {
    mockPost.mockRejectedValue({
      response: { data: { message: 'Result budget exceeded. Add LIMIT.' } },
    });
    const user = userEvent.setup();
    renderWithProviders(<QueryPage />);
    await user.click(screen.getByRole('button', { name: 'query.execute' }));
    expect(await screen.findByText('Result budget exceeded. Add LIMIT.')).toBeInTheDocument();
  });

  it('does not submit another query from keyboard shortcuts while one is running', async () => {
    mockPost.mockReturnValue(new Promise(() => {}));
    const user = userEvent.setup();
    renderWithProviders(<QueryPage />);
    await user.click(screen.getByRole('button', { name: 'query.execute' }));
    fireEvent.keyDown(window, { key: 'Enter', ctrlKey: true });
    expect(mockPost).toHaveBeenCalledTimes(1);
  });

  it('isolates persisted SQL history by both tenant and user without copying old entries', () => {
    const firstUser = {
      id: 'analyst',
      tenant_id: 'tenant-a',
      username: 'analyst',
      email: 'a@example.invalid',
      tenant_name: 'A',
      roles: [],
      permissions: [],
    };
    const otherUser = { ...firstUser, id: 'other-user' };
    const otherTenant = { ...firstUser, tenant_id: 'tenant-b' };
    const entries = [
      { user: firstUser, sql: 'SELECT tenant_a_secret' },
      { user: otherUser, sql: 'SELECT other_user_secret' },
      { user: otherTenant, sql: 'SELECT tenant_b_secret' },
    ];
    for (const entry of entries)
      localStorage.setItem(
        queryHistoryKey(entry.user)!,
        JSON.stringify([{ sql: entry.sql, timestamp: 1 }]),
      );
    useAuthStore.setState({ user: firstUser });
    renderWithProviders(<QueryPage />);
    expect(screen.getByText('SELECT tenant_a_secret')).toBeInTheDocument();
    act(() => {
      useAuthStore.setState({ user: otherUser });
    });
    expect(screen.queryByText('SELECT tenant_a_secret')).not.toBeInTheDocument();
    expect(screen.getByText('SELECT other_user_secret')).toBeInTheDocument();
    act(() => {
      useAuthStore.setState({ user: otherTenant });
    });
    expect(screen.queryByText('SELECT other_user_secret')).not.toBeInTheDocument();
    expect(screen.getByText('SELECT tenant_b_secret')).toBeInTheDocument();
    expect(JSON.parse(localStorage.getItem(queryHistoryKey(otherTenant)!)!)).toEqual([
      { sql: 'SELECT tenant_b_secret', timestamp: 1 },
    ]);
  });
});
