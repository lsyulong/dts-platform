import { useTranslation } from 'react-i18next';
import { Database } from 'lucide-react';
import { EmptyState } from '../../../components/ui';

export function MappingStep() {
  const { t } = useTranslation();
  return (
    <EmptyState
      icon={Database}
      title={t('tasks.mappingConfig')}
      description={t('tasks.mappingHint')}
    />
  );
}
