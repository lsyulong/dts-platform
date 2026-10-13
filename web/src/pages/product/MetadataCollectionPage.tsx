import { useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import type { ProductModule } from '../../product/catalog-types';
import ProductModulePage from '../../product/ProductModulePage';
import JobRunsPage from './JobRunsPage';

export default function MetadataCollectionPage({ module }: { module: ProductModule }) {
  const { i18n } = useTranslation();
  const [tab, setTab] = useState<'definitions' | 'runs'>('definitions');
  const [params, setParams] = useSearchParams();
  const active = params.has('job') ? 'runs' : tab;
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  return (
    <div className="space-y-5">
      <div
        className="flex gap-4 border-b border-[var(--color-border-default)]"
        role="tablist"
        aria-label={text('元数据采集', 'Metadata collection')}
      >
        {(['definitions', 'runs'] as const).map((value) => (
          <button
            key={value}
            role="tab"
            aria-selected={active === value}
            onClick={() => {
              setTab(value);
              const next = new URLSearchParams(params);
              next.delete('job');
              setParams(next);
            }}
            className={`border-b-2 px-3 py-3 text-sm ${active === value ? 'border-[var(--color-accent)] text-[var(--color-accent)]' : 'border-transparent text-[var(--color-text-secondary)]'}`}
          >
            {value === 'definitions'
              ? text('采集配置', 'Collection definitions')
              : text('运行记录', 'Run history')}
          </button>
        ))}
      </div>
      {active === 'definitions' ? (
        <ProductModulePage module={module} />
      ) : (
        <JobRunsPage scope="metadata" />
      )}
    </div>
  );
}
