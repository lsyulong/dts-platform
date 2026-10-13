import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { ProductModule } from '../../product/catalog-types';
import ProductModulePage from '../../product/ProductModulePage';
import ServiceCredentialsPanel from './ServiceCredentialsPanel';

export default function ServiceApplicationsPage({ module }: { module: ProductModule }) {
  const { i18n } = useTranslation(),
    [tab, setTab] = useState<'applications' | 'credentials'>('applications');
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  return (
    <div className="space-y-5">
      <div
        role="tablist"
        aria-label={text('服务应用', 'Service applications')}
        className="flex gap-4 border-b border-[var(--color-border-default)]"
      >
        {(['applications', 'credentials'] as const).map((value) => (
          <button
            role="tab"
            key={value}
            aria-selected={tab === value}
            onClick={() => setTab(value)}
            className={`border-b-2 px-3 py-3 text-sm ${tab === value ? 'border-[var(--color-accent)] text-[var(--color-accent)]' : 'border-transparent text-[var(--color-text-secondary)]'}`}
          >
            {value === 'applications'
              ? text('应用身份', 'Applications')
              : text('调用凭据', 'Credentials')}
          </button>
        ))}
      </div>
      {tab === 'applications' ? <ProductModulePage module={module} /> : <ServiceCredentialsPanel />}
    </div>
  );
}
