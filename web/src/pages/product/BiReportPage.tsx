import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useSearchParams } from 'react-router-dom';
import type { ProductModule } from '../../product/catalog-types';
import ProductModulePage from '../../product/ProductModulePage';
import JobRunsPage from './JobRunsPage';
import BiSharesPage from './BiSharesPage';

export default function BiReportPage({ module }: { module: ProductModule }) {
  const { i18n } = useTranslation();
  const [tab, setTab] = useState<'definitions' | 'runs' | 'shares'>('definitions');
  const [params, setParams] = useSearchParams();
  const activeTab = params.get('share') ? 'shares' : params.has('job') ? 'runs' : tab;
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  return (
    <div className="space-y-5">
      <div
        className="flex gap-4 border-b border-[var(--color-border-default)]"
        role="tablist"
        aria-label={text('BI 分析', 'BI analysis')}
      >
        {(['definitions', 'runs', 'shares'] as const).map((value) => (
          <button
            key={value}
            role="tab"
            aria-selected={activeTab === value}
            onClick={() => {
              setTab(value);
              const next = new URLSearchParams(params);
              next.delete('share');
              next.delete('job');
              setParams(next);
            }}
            className={`border-b-2 px-3 py-3 text-sm ${activeTab === value ? 'border-[var(--color-accent)] text-[var(--color-accent)]' : 'border-transparent text-[var(--color-text-secondary)]'}`}
          >
            {value === 'definitions'
              ? text('报表定义', 'Report definitions')
              : value === 'runs'
                ? text('分析结果', 'Analysis results')
                : text('报表分享', 'Report shares')}
          </button>
        ))}
      </div>
      {activeTab === 'definitions' ? (
        <ProductModulePage module={module} />
      ) : activeTab === 'runs' ? (
        <JobRunsPage scope="bi" />
      ) : (
        <BiSharesPage />
      )}
    </div>
  );
}
