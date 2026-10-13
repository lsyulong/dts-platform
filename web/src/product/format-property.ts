export function formatProperty(value: unknown, language: string): string {
  if (value === null || value === undefined || value === '') return '—';
  if (typeof value === 'boolean')
    return language.startsWith('zh') ? (value ? '是' : '否') : value ? 'Yes' : 'No';
  if (Array.isArray(value)) return `${value.length} ${language.startsWith('zh') ? '项' : 'items'}`;
  if (typeof value === 'object') return JSON.stringify(value);
  return String(value);
}
