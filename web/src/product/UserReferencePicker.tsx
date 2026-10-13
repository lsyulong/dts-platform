import { useDeferredValue, useId, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import client from '../api/client';
import { Button } from '../components/ui/button';
import { ErrorBanner } from '../components/ui';
import { apiErrorMessage } from '../lib/apiError';
import { useAuthStore } from '../stores/auth';
import { selectionSessionScope } from './business-editors/option-sources';

interface UserOption {
  id: string;
  tenant_id: string;
  username: string;
  display_name: string | null;
}
interface UserPage {
  data: UserOption[];
  total: number;
}

export default function UserReferencePicker({
  label,
  value,
  onChange,
  disabled,
  required,
  excludeId,
  resourceKind,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
  required?: boolean;
  excludeId?: string;
  resourceKind?: string;
}) {
  const id = useId();
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const user = useAuthStore((state) => state.user);
  const token = useAuthStore((state) => state.token);
  const [search, setSearch] = useState('');
  const [offset, setOffset] = useState(0);
  const q = useDeferredValue(search.trim());
  const query = useQuery({
    queryKey: [
      'product-reference-users',
      user?.tenant_id,
      user?.id,
      selectionSessionScope(token),
      resourceKind,
      q,
      offset,
    ],
    queryFn: async ({ signal }) => {
      const response = await client.get<UserPage>('/product/reference/users', {
        params: { resource_kind: resourceKind, q, limit: 50, offset },
        signal,
        headers: { Authorization: `Bearer ${token}` },
      });
      const current = useAuthStore.getState();
      if (
        current.token !== token ||
        current.user?.id !== user?.id ||
        current.user?.tenant_id !== user?.tenant_id
      )
        throw new Error(
          text('登录身份已变化，请重新读取', 'Your session changed; reload this list'),
        );
      if (
        !Array.isArray(response.data.data) ||
        !Number.isSafeInteger(response.data.total) ||
        response.data.total < 0 ||
        response.data.data.some(
          (row) =>
            row.tenant_id !== user?.tenant_id ||
            typeof row.id !== 'string' ||
            typeof row.username !== 'string',
        )
      )
        throw new Error(text('用户目录返回了无效身份', 'Invalid user directory response'));
      return response.data;
    },
    enabled: !!user && !!token && !!resourceKind,
  });
  const options = query.data?.data.filter((option) => option.id !== excludeId) ?? [];
  return (
    <div className="space-y-2">
      <label htmlFor={id} className="block text-sm font-medium text-[var(--color-text-secondary)]">
        {label}
        {required && <span className="ml-1 text-[var(--color-error)]">*</span>}
      </label>
      <input
        className="w-full rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-2 text-sm"
        aria-label={text('查找租户用户', 'Search tenant users')}
        placeholder={text('姓名或账号', 'Name or username')}
        value={search}
        disabled={disabled}
        onChange={(event) => {
          setSearch(event.target.value);
          setOffset(0);
        }}
      />
      <select
        id={id}
        className="w-full rounded-md border border-[var(--color-border-hover)] bg-[var(--color-bg-elevated)] p-2 text-sm"
        disabled={disabled || query.isPending}
        value={value}
        required={required}
        onChange={(event) => onChange(event.target.value)}
      >
        <option value="">{text('选择有效租户用户', 'Select an active tenant user')}</option>
        {value && !options.some((option) => option.id === value) && (
          <option value={value}>{value}</option>
        )}
        {options.map((option) => (
          <option key={option.id} value={option.id}>
            {option.display_name || option.username} · {option.username}
          </option>
        ))}
      </select>
      {query.error && (
        <ErrorBanner
          message={apiErrorMessage(query.error, text('用户读取失败', 'Could not load users'))}
        />
      )}
      <div className="flex justify-end gap-2">
        <Button
          type="button"
          variant="ghost"
          size="sm"
          disabled={disabled || offset === 0 || query.isFetching}
          onClick={() => setOffset(Math.max(0, offset - 50))}
        >
          {text('上一页', 'Previous')}
        </Button>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          disabled={disabled || !query.data || offset + 50 >= query.data.total || query.isFetching}
          onClick={() => setOffset(offset + 50)}
        >
          {text('下一页', 'Next')}
        </Button>
      </div>
    </div>
  );
}
