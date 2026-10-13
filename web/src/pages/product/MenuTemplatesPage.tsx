import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '../../components/ui/button';
import type { ProductModule } from '../../product/catalog-types';
import ProductModulePage from '../../product/ProductModulePage';
import NavigationPolicyPanel from './NavigationPolicyPanel';

export default function MenuTemplatesPage({ module }: { module: ProductModule }) {
  const { i18n } = useTranslation();
  const [tab, setTab] = useState<'definitions' | 'policy'>('definitions');
  const zh = i18n.language.startsWith('zh');
  return (
    <div className="space-y-5">
      <div className="flex gap-2 border-b border-[var(--color-border-default)] pb-3">
        <Button
          variant={tab === 'definitions' ? 'default' : 'outline'}
          onClick={() => setTab('definitions')}
        >
          {zh ? '模板定义' : 'Template definitions'}
        </Button>
        <Button variant={tab === 'policy' ? 'default' : 'outline'} onClick={() => setTab('policy')}>
          {zh ? '当前租户菜单与生效历史' : 'Tenant navigation and history'}
        </Button>
      </div>
      {tab === 'definitions' ? <ProductModulePage module={module} /> : <NavigationPolicyPanel />}
    </div>
  );
}
