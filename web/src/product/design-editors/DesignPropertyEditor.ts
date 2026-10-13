import { createElement } from 'react';
import ApiResponseSchemaEditor from './ApiResponseSchemaEditor';
import ChartConfigEditor from './ChartConfigEditor';
import PortalLayoutEditor from './PortalLayoutEditor';
import PortalThemeEditor from './PortalThemeEditor';
import ReportDimensionsEditor from './ReportDimensionsEditor';
import ReportMeasuresEditor from './ReportMeasuresEditor';
import type { DesignEditorProps } from './design-models';

type DesignKind =
  'portal_layout' | 'portal_theme' | 'chart' | 'dimensions' | 'measures' | 'response_schema';
function editorKind(kind: string, key: string): DesignKind | null {
  if (kind === 'portal' && key === 'layout') return 'portal_layout';
  if (kind === 'portal' && key === 'theme') return 'portal_theme';
  if (kind === 'bi_report' && key === 'chart_config') return 'chart';
  if (kind === 'bi_report' && key === 'dimensions') return 'dimensions';
  if (kind === 'bi_report' && key === 'measures') return 'measures';
  if (kind === 'data_api' && key === 'response_schema') return 'response_schema';
  return null;
}
export const hasDesignPropertyEditor = (kind: string, fieldKey: string) =>
  editorKind(kind, fieldKey) !== null;
export default function DesignPropertyEditor({
  kind,
  fieldKey,
  ...props
}: DesignEditorProps & { kind: string; fieldKey: string }) {
  switch (editorKind(kind, fieldKey)) {
    case 'portal_layout':
      return createElement(PortalLayoutEditor, props);
    case 'portal_theme':
      return createElement(PortalThemeEditor, props);
    case 'chart':
      return createElement(ChartConfigEditor, props);
    case 'dimensions':
      return createElement(ReportDimensionsEditor, props);
    case 'measures':
      return createElement(ReportMeasuresEditor, props);
    case 'response_schema':
      return createElement(ApiResponseSchemaEditor, props);
    default:
      return null;
  }
}
