import { useTranslation } from 'react-i18next';
import { Sparkles } from 'lucide-react';

/** AI 分析页 — DeerFlow 智能分析工作台 (Phase 3 P0 实现) */
export default function AnalysisPage() {
  const { t } = useTranslation();
  return (
    <div className="flex flex-1 items-center justify-center">
      <div className="text-center">
        <div className="mx-auto mb-4 flex h-16 w-16 items-center justify-center rounded-2xl bg-violet-500/10">
          <Sparkles className="text-violet-400" size={32} />
        </div>
        <h1 className="text-xl font-semibold text-[var(--color-text-primary)]">{t('nav.analysis')}</h1>
        <p className="mt-2 text-sm text-[var(--color-text-tertiary)]">
          DeerFlow AI — NL2SQL · 对话式查询 · 智能报告生成
        </p>
        <p className="mt-1 text-xs text-[var(--color-text-disabled)]">Phase 3 P0 实现</p>
      </div>
    </div>
  );
}
