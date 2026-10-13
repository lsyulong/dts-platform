export interface BusinessPropertyEditorProps {
  kind: string;
  fieldKey: string;
  value: unknown;
  onChange: (value: unknown) => void;
  disabled?: boolean;
  properties?: Record<string, unknown>;
}

export interface ColumnMapping {
  source_column: string;
  target_column: string;
}

export interface SqlParameter {
  name: string;
  data_type: string;
  required: boolean;
  default_value?: string | number | boolean | null;
}

export interface SelectionOption {
  id: string;
  label: string;
  detail: string;
}
