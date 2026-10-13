import type { SensitiveScanResult } from './SensitiveRunResult';
import type { SensitiveColumnEvidence } from './sensitive-api';
function object(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
const count = (value: unknown): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= 0;
function column(value: unknown): SensitiveColumnEvidence | null {
  if (
    !object(value) ||
    typeof value.column_name !== 'string' ||
    typeof value.data_type !== 'string' ||
    !count(value.checked_values) ||
    !count(value.matched_values) ||
    !count(value.null_values) ||
    value.matched_values > value.checked_values
  )
    return null;
  return {
    column_name: value.column_name,
    data_type: value.data_type,
    checked_values: value.checked_values,
    matched_values: value.matched_values,
    null_values: value.null_values,
  };
}
export function committedSensitiveResult(value: unknown): SensitiveScanResult | null {
  if (
    !object(value) ||
    value.producer_committed !== true ||
    'rows' in value ||
    !['sample', 'column_names'].includes(String(value.evaluation_scope)) ||
    !count(value.sample_limit) ||
    value.sample_limit < 1 ||
    value.sample_limit > 1000 ||
    !count(value.sampled_rows) ||
    value.sampled_rows > value.sample_limit ||
    typeof value.definition_hash !== 'string' ||
    !/^[a-f0-9]{64}$/.test(value.definition_hash) ||
    !Array.isArray(value.finding_ids) ||
    !value.finding_ids.every((id): id is string => typeof id === 'string') ||
    !Array.isArray(value.columns) ||
    !value.columns.length ||
    value.columns.length > 128
  )
    return null;
  const columns = value.columns.map(column);
  if (columns.some((c) => c === null)) return null;
  const checked = columns.filter((c): c is SensitiveColumnEvidence => c !== null);
  const scope = value.evaluation_scope === 'sample' ? 'sample' : 'column_names';
  if (
    new Set(checked.map((c) => c.column_name)).size !== checked.length ||
    (scope === 'column_names' && value.sampled_rows !== 0) ||
    checked.some(
      (c) => c.checked_values + c.null_values !== (scope === 'sample' ? value.sampled_rows : 1),
    )
  )
    return null;
  return {
    evaluation_scope: scope,
    sample_limit: value.sample_limit,
    sampled_rows: value.sampled_rows,
    columns: checked,
    definition_hash: value.definition_hash,
    producer_committed: true,
    finding_ids: value.finding_ids,
  };
}
