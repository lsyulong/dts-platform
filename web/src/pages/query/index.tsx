import { useTranslation } from 'react-i18next';
import { Search } from 'lucide-react';

/** 数据查询页 — SwanLake SQL 工作台 (Phase 3 P2 实现) */
export default function QueryPage() {
  const { t } = useTranslation();
  return (
    <div className="flex flex-1 items-center justify-center">
      <div className="text-center">
        <div className="mx-auto mb-4 flex h-16 w-16 items-center justify-center rounded-2xl bg-blue-500/10">
          <Search className="text-blue-400" size={32} />
        </div>
        <h1 className="text-xl font-semibold text-[var(--color-text-primary)]">{t('nav.query')}</h1>
        <p className="mt-2 text-sm text-[var(--color-text-tertiary)]">
          SwanLake Arrow Flight SQL — 高性能数据查询工作台
        </p>
        <p className="mt-1 text-xs text-[var(--color-text-disabled)]">Phase 3 P2 实现</p>
      </div>
    </div>
  );
}
