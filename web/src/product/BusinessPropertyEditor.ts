import { createElement } from 'react';
import type { BusinessPropertyEditorProps } from './business-editors/types';
import StringListEditor from './business-editors/StringListEditor';
import IdMultiSelectEditor from './business-editors/IdMultiSelectEditor';
import ColumnMappingEditor from './business-editors/ColumnMappingEditor';
import KeyValueObjectEditor from './business-editors/KeyValueObjectEditor';
import SqlParametersEditor from './business-editors/SqlParametersEditor';
import AudienceEditor from './business-editors/AudienceEditor';
import MasterValuesEditor from './business-editors/MasterValuesEditor';
import MetadataColumnsEditor from './business-editors/MetadataColumnsEditor';

export type { BusinessPropertyEditorProps } from './business-editors/types';

const listKeys = new Set(['primary_key', 'key_fields', 'dimensions', 'tags', 'allowed_origins']);
const selectionKeys = new Set([
  'member_ids',
  'recipient_ids',
  'authorized_role_ids',
  'menu_ids',
  'rule_ids',
  'asset_ids',
  'metric_ids',
]);

function editorFor(kind: string, fieldKey: string) {
  if (kind === 'announcement' && fieldKey === 'audience') return AudienceEditor;
  if (kind === 'master_data_record' && fieldKey === 'values') return MasterValuesEditor;
  if (fieldKey === 'columns' && ['data_access_request', 'quality_object'].includes(kind))
    return MetadataColumnsEditor;
  if (selectionKeys.has(fieldKey)) return IdMultiSelectEditor;
  if (
    listKeys.has(fieldKey) ||
    (fieldKey === 'columns' && ['data_access_request', 'quality_object'].includes(kind)) ||
    (kind === 'message_template' && fieldKey === 'variables')
  )
    return StringListEditor;
  if (fieldKey === 'mapping' && ['realtime_task', 'master_distribution'].includes(kind))
    return ColumnMappingEditor;
  if (
    (fieldKey === 'variables' && kind === 'development_config') ||
    (fieldKey === 'parameters' && ['system_message', 'masking_rule'].includes(kind))
  )
    return KeyValueObjectEditor;
  if (fieldKey === 'parameters' && ['sql_document', 'data_api'].includes(kind))
    return SqlParametersEditor;
  return null;
}

export function hasBusinessPropertyEditor(kind: string, fieldKey: string) {
  return editorFor(kind, fieldKey) !== null;
}

export default function BusinessPropertyEditor(props: BusinessPropertyEditorProps) {
  const Editor = editorFor(props.kind, props.fieldKey);
  return Editor ? createElement(Editor, props) : null;
}
