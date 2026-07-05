import { describe, it, expect, beforeAll, afterAll, afterEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';
import { server } from '../../test/mocks/server';
import LoginPage from '../login';

describe('LoginPage', () => {
  beforeAll(() => server.listen());
  afterEach(() => {
    server.resetHandlers();
    localStorage.clear();
  });
  afterAll(() => server.close());

  function renderLogin() {
    return render(
      <MemoryRouter initialEntries={['/login']}>
        <LoginPage />
      </MemoryRouter>
    );
  }

  it('renders the login form with DTS Platform title', () => {
    renderLogin();
    expect(screen.getByText('DTS Platform')).toBeInTheDocument();
    expect(screen.getByText('企业级数据同步管理平台')).toBeInTheDocument();
  });

  it('renders tenant identifier input with default value', () => {
    renderLogin();
    const tenantInput = screen.getByPlaceholderText('default') as HTMLInputElement;
    expect(tenantInput).toBeInTheDocument();
    expect(tenantInput.value).toBe('default');
  });

  it('renders username input', () => {
    renderLogin();
    expect(screen.getByPlaceholderText('admin')).toBeInTheDocument();
  });

  it('renders password input', () => {
    renderLogin();
    expect(screen.getByPlaceholderText('••••••••')).toBeInTheDocument();
  });

  it('renders submit button with "登录" text', () => {
    renderLogin();
    expect(screen.getByRole('button', { name: '登录' })).toBeInTheDocument();
  });

  it('renders default credentials hint', () => {
    renderLogin();
    expect(screen.getByText(/默认账户: admin \/ admin123/)).toBeInTheDocument();
  });

  it('allows user to fill in the form fields', async () => {
    const user = userEvent.setup();
    renderLogin();

    const tenantInput = screen.getByPlaceholderText('default');
    const usernameInput = screen.getByPlaceholderText('admin');
    const passwordInput = screen.getByPlaceholderText('••••••••');

    await user.clear(tenantInput);
    await user.type(tenantInput, 'my-tenant');
    await user.type(usernameInput, 'admin');
    await user.type(passwordInput, 'admin123');

    expect(tenantInput).toHaveValue('my-tenant');
    expect(usernameInput).toHaveValue('admin');
    expect(passwordInput).toHaveValue('admin123');
  });

  it('does not show error after successful login attempt', async () => {
    const user = userEvent.setup();
    renderLogin();

    await user.type(screen.getByPlaceholderText('admin'), 'admin');
    await user.type(screen.getByPlaceholderText('••••••••'), 'admin123');

    await user.click(screen.getByRole('button', { name: '登录' }));

    // After successful login, should not show error
    await waitFor(() => {
      expect(screen.queryByText('用户名或密码错误')).not.toBeInTheDocument();
    });
  });

  it('displays error message on login failure', async () => {
    const user = userEvent.setup();
    renderLogin();

    await user.type(screen.getByPlaceholderText('admin'), 'wrong');
    await user.type(screen.getByPlaceholderText('••••••••'), 'wrong');
    await user.click(screen.getByRole('button', { name: '登录' }));

    await waitFor(() => {
      expect(screen.getByText('用户名或密码错误')).toBeInTheDocument();
    });
  });

  it('does not show error initially', () => {
    renderLogin();
    expect(screen.queryByText('用户名或密码错误')).not.toBeInTheDocument();
  });
});
