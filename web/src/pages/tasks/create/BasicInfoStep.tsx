import { useTranslation } from 'react-i18next';
import { FormInput } from '../../../components/ui';

type Props = {
  name: string;
  description: string;
  onNameChange: (v: string) => void;
  onDescriptionChange: (v: string) => void;
};

export function BasicInfoStep({ name, description, onNameChange, onDescriptionChange }: Props) {
  const { t } = useTranslation();
  return (
    <div className="space-y-4">
      <FormInput
        label={t('tasks.taskName')}
        value={name}
        onChange={onNameChange}
        placeholder={t('tasks.namePlaceholder')}
      />
      <FormInput
        label={t('tasks.descPlaceholder')}
        type="textarea"
        value={description}
        onChange={onDescriptionChange}
        placeholder={t('tasks.descPlaceholder')}
        rows={3}
      />
    </div>
  );
}
