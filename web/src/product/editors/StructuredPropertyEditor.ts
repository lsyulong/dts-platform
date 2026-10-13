import { createElement } from 'react';
import ModelColumnsEditor from './ModelColumnsEditor';
import EnumValuesEditor from './EnumValuesEditor';
import RuleConditionsEditor from './RuleConditionsEditor';
import WorkflowGraphEditor from './WorkflowGraphEditor';
import type { StructuredEditorProps } from './editor-models';

type EditorKind = 'columns' | 'enum_values' | 'conditions' | 'definition';

function editorKind(kind: string, fieldKey: string): EditorKind | null {
  if ((kind === 'data_model' || kind === 'master_data_model') && fieldKey === 'columns')
    return 'columns';
  if ((kind === 'enum_standard' || kind === 'system_dictionary') && fieldKey === 'enum_values')
    return 'enum_values';
  if (kind === 'quality_rule' && fieldKey === 'conditions') return 'conditions';
  if (kind === 'development_workflow' && fieldKey === 'definition') return 'definition';
  return null;
}

export function hasStructuredPropertyEditor(kind: string, fieldKey: string): boolean {
  return editorKind(kind, fieldKey) !== null;
}

export default function StructuredPropertyEditor({
  fieldKey,
  kind,
  ...props
}: StructuredEditorProps & { fieldKey: string; kind: string }) {
  switch (editorKind(kind, fieldKey)) {
    case 'columns':
      return createElement(ModelColumnsEditor, props);
    case 'enum_values':
      return createElement(EnumValuesEditor, props);
    case 'conditions':
      return createElement(RuleConditionsEditor, props);
    case 'definition':
      return createElement(WorkflowGraphEditor, props);
    default:
      return null;
  }
}
