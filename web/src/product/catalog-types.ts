export interface ProductText {
  zh: string;
  en: string;
}

export interface ProductGroup {
  id: string;
  title: ProductText;
  description: ProductText;
}

export interface ResourceField {
  key: string;
  label: ProductText;
  type: 'text' | 'textarea' | 'number' | 'boolean' | 'select' | 'json';
  required?: boolean;
  readOnly?: boolean;
  referenceKind?: string;
  options?: { value: string; label: ProductText }[];
}

export interface ProductModule {
  id: string;
  groupId: string;
  title: ProductText;
  description: ProductText;
  path: string;
  reference: string[];
  resourceKind?: string;
  existingPath?: string;
  permission?: string;
  implementation: 'existing' | 'new';
  workspaceType?: 'table' | 'tree' | 'editor' | 'workflow' | 'review' | 'monitor' | 'catalog';
  fields?: ResourceField[];
  actions?: string[];
}

export const productText = (zh: string, en: string): ProductText => ({ zh, en });

export const resourceField = (
  key: string,
  zh: string,
  en: string,
  type: ResourceField['type'] = 'text',
  required = false,
): ResourceField => ({ key, label: productText(zh, en), type, required });

export const referenceField = (
  key: string,
  zh: string,
  en: string,
  referenceKind: string,
  required = true,
): ResourceField => ({ ...resourceField(key, zh, en, 'text', required), referenceKind });

export const selectField = (
  key: string,
  zh: string,
  en: string,
  options: [string, string, string][],
  required = true,
): ResourceField => ({
  ...resourceField(key, zh, en, 'select', required),
  options: options.map(([value, labelZh, labelEn]) => ({
    value,
    label: productText(labelZh, labelEn),
  })),
});
