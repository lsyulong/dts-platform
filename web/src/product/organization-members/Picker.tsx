import { useId, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '../../components/ui/button';
import { ErrorBanner } from '../../components/ui';
import { apiErrorMessage } from '../../lib/apiError';
import type { ResourcePage } from '../resource-types';
import { useOrganizationQuery } from './api';

interface UserReference {
  id: string;
  tenant_id: string;
  username: string;
  display_name: string | null;
}
export default function OrganizationReferencePicker({
  kind,
  label,
  value,
  onChange,
  disabled = false,
  excludeId,
}: {
  kind: 'user' | 'organization';
  label: string;
  value: string;
  onChange: (id: string) => void;
  disabled?: boolean;
  excludeId?: string;
}) {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const id = useId();
  const [q, setQ] = useState(''),
    [offset, setOffset] = useState(0);
  const users = useOrganizationQuery<{ data: UserReference[]; total: number }>(
    '/product/reference/users',
    { resource_kind: 'organization', q, limit: 20, offset },
    kind === 'user',
    (page, tenant) => {
      if (page.data.some((user) => user.tenant_id !== tenant))
        throw new Error('Invalid user directory');
    },
  );
  const orgs = useOrganizationQuery<ResourcePage>(
    '/product/resources/organization',
    { q, limit: 20, offset },
    kind === 'organization',
    (page, tenant) => {
      if (page.data.some((org) => org.tenant_id !== tenant))
        throw new Error('Invalid organization directory');
    },
  );
  const query = kind === 'user' ? users : orgs;
  const options =
    kind === 'user'
      ? (users.isError || users.isFetching ? [] : (users.data?.data ?? [])).map((user) => ({
          id: user.id,
          name: user.display_name || user.username,
        }))
      : (orgs.isError || orgs.isFetching ? [] : (orgs.data?.data ?? []))
          .filter((org) => org.status === 'draft' || org.status === 'published')
          .map((org) => ({ id: org.id, name: org.name }));
  return (
    <div className="space-y-2">
      <label htmlFor={id} className="block text-sm">
        {label}
      </label>
      <input
        aria-label={`${label}搜索`}
        value={q}
        maxLength={128}
        disabled={disabled}
        placeholder={text('搜索当前租户', 'Search current tenant')}
        onChange={(event) => {
          setQ(event.target.value);
          setOffset(0);
        }}
        className="w-full rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-2 text-sm"
      />
      <select
        id={id}
        value={value}
        disabled={disabled || query.isFetching}
        onChange={(event) => onChange(event.target.value)}
        className="w-full rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-2 text-sm"
      >
        <option value="">{text('请选择', 'Choose')}</option>
        {value && !options.some((option) => option.id === value) && (
          <option value={value}>{value}</option>
        )}
        {options
          .filter((option) => option.id !== excludeId)
          .map((option) => (
            <option value={option.id} key={option.id}>
              {option.name}
            </option>
          ))}
      </select>
      {query.error && (
        <ErrorBanner
          message={apiErrorMessage(query.error, text('目录加载失败', 'Directory failed to load'))}
        />
      )}
      <div className="flex items-center justify-between gap-2 text-xs">
        <span>
          {text('共', 'Total')}{' '}
          {query.isError || query.isFetching ? '—' : (query.data?.total ?? '—')}
        </span>
        <div className="flex gap-2">
          <Button
            type="button"
            size="sm"
            variant="ghost"
            disabled={disabled || !offset || query.isFetching}
            onClick={() => setOffset(Math.max(0, offset - 20))}
          >
            {text('上一页', 'Previous')}
          </Button>
          <Button
            type="button"
            size="sm"
            variant="ghost"
            disabled={
              disabled ||
              !query.data ||
              offset + 20 >= query.data.total ||
              query.isFetching ||
              query.isError
            }
            onClick={() => setOffset(offset + 20)}
          >
            {text('下一页', 'Next')}
          </Button>
        </div>
      </div>
    </div>
  );
}
