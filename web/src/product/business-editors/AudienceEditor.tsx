import { useTranslation } from 'react-i18next';
import IdMultiSelectEditor from './IdMultiSelectEditor';
import type { BusinessPropertyEditorProps } from './types';
import { isRecord } from './business-models';
import { inputClass } from './business-ui';

const audienceFields: Record<string, string> = {
  users: 'user_ids',
  roles: 'role_ids',
  organizations: 'organization_ids',
};

export default function AudienceEditor({ value, onChange, disabled }: BusinessPropertyEditorProps) {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const audience = isRecord(value) ? value : {};
  const type = typeof audience.type === 'string' ? audience.type : '';
  const field = audienceFields[type];
  return (
    <div className="space-y-3">
      <label className="block text-sm">
        {text('接收范围', 'Audience')}
        <select
          className={`${inputClass} mt-2`}
          value={type}
          required
          disabled={disabled}
          onChange={(event) => {
            const next = event.target.value;
            onChange(
              next === 'tenant' ? { type: next } : { type: next, [audienceFields[next]]: [] },
            );
          }}
        >
          <option value="">{text('选择接收范围', 'Select audience')}</option>
          <option value="tenant">{text('租户内所有用户', 'Everyone in this tenant')}</option>
          <option value="users">{text('指定用户', 'Selected users')}</option>
          <option value="roles">{text('指定角色', 'Selected roles')}</option>
          <option value="organizations">
            {text('指定组织成员', 'Selected organization members')}
          </option>
        </select>
      </label>
      {field && (
        <IdMultiSelectEditor
          kind="announcement"
          fieldKey={field}
          value={audience[field]}
          onChange={(ids) => onChange({ type, [field]: ids })}
          disabled={disabled}
        />
      )}
    </div>
  );
}
