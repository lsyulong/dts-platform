import type { StructuredEditorProps } from '../editors/editor-models';

export interface DesignEditorProps extends StructuredEditorProps {
  properties?: Record<string, unknown>;
}

export const widgetTypes = [
  'metric',
  'task_list',
  'datasource_list',
  'announcement',
  'chart',
  'shortcut',
] as const;
export type WidgetType = (typeof widgetTypes)[number];
export interface PortalWidget {
  id: string;
  type: WidgetType;
  title: string;
  module_id: string;
  width: number;
  height: number;
}
export interface PortalLayout {
  columns: 12;
  widgets: PortalWidget[];
}
export interface PortalTheme {
  primary_color: string;
  background_color: string;
  title: string;
}
export const chartTypes = ['bar', 'line', 'pie', 'table'] as const;
export type ChartType = (typeof chartTypes)[number];
export interface ChartConfig {
  type: ChartType;
  title: string;
  x_field: string;
  y_field: string;
  legend: boolean;
}
export const aggregates = ['sum', 'avg', 'count', 'min', 'max'] as const;
export type Aggregate = (typeof aggregates)[number];
export interface ReportMeasure {
  name: string;
  expression: string;
  aggregate: Aggregate;
}
export const maxWidgets = 48;

export function record(value: unknown): Record<string, unknown> {
  return value && typeof value === 'object' && !Array.isArray(value)
    ? Object.fromEntries(Object.entries(value))
    : {};
}
const text = (value: unknown) => (typeof value === 'string' ? value : '');
const items = (value: unknown): unknown[] => (Array.isArray(value) ? value : []);
export const safeHex = (value: unknown): value is string =>
  typeof value === 'string' && /^#[0-9a-f]{6}$/i.test(value);
export function boundedInteger(value: unknown, min: number, max: number, fallback = min): number {
  return typeof value === 'number' && Number.isFinite(value)
    ? Math.max(min, Math.min(max, Math.round(value)))
    : fallback;
}

export function portalLayout(value: unknown): PortalLayout {
  const layout = record(value);
  return {
    columns: 12,
    widgets: items(layout.widgets).map((value) => {
      const row = record(value);
      return {
        id: text(row.id),
        type: widgetTypes.find((type) => type === row.type) ?? 'shortcut',
        title: text(row.title),
        module_id: text(row.module_id),
        width: boundedInteger(row.width, 1, 12, 6),
        height: boundedInteger(row.height, 1, 8, 2),
      };
    }),
  };
}
export function createPortalWidget(widgets: PortalWidget[]): PortalWidget {
  const used = new Set(widgets.map((widget) => widget.id));
  let next = 1;
  while (used.has(`widget-${next}`)) next += 1;
  return { id: `widget-${next}`, type: 'shortcut', title: '', module_id: '', width: 6, height: 2 };
}
export function moveWidget(layout: PortalLayout, index: number, direction: -1 | 1): PortalLayout {
  const next = index + direction;
  if (index < 0 || index >= layout.widgets.length || next < 0 || next >= layout.widgets.length)
    return layout;
  const widgets = [...layout.widgets];
  const current = widgets[index];
  widgets[index] = widgets[next];
  widgets[next] = current;
  return { columns: 12, widgets };
}
export function portalTheme(value: unknown): PortalTheme {
  const theme = record(value);
  return {
    primary_color: typeof theme.primary_color === 'string' ? theme.primary_color : '#0D9488',
    background_color:
      typeof theme.background_color === 'string' ? theme.background_color : '#F4F7FA',
    title: text(theme.title),
  };
}
export function chartConfig(value: unknown): ChartConfig {
  const config = record(value);
  return {
    type: chartTypes.find((type) => type === config.type) ?? 'bar',
    title: text(config.title),
    x_field: text(config.x_field),
    y_field: text(config.y_field),
    legend: config.legend !== false,
  };
}
export function reportDimensions(value: unknown): string[] {
  return items(value).map(text);
}
export function reportMeasures(value: unknown): ReportMeasure[] {
  return items(value).map((value) => {
    const row = record(value);
    return {
      name: text(row.name),
      expression: text(row.expression),
      aggregate: aggregates.find((aggregate) => aggregate === row.aggregate) ?? 'sum',
    };
  });
}
export function duplicateNames(values: string[]): string[] {
  const normalized = values.map((value) => value.trim().toLowerCase()).filter(Boolean);
  return [...new Set(normalized.filter((value, index) => normalized.indexOf(value) !== index))];
}
const isIntegerBetween = (value: unknown, min: number, max: number) =>
  typeof value === 'number' && Number.isInteger(value) && value >= min && value <= max;
const onlyKeys = (value: Record<string, unknown>, keys: string[]) =>
  Object.keys(value).every((key) => keys.includes(key));
const validTitle = (value: unknown) =>
  typeof value === 'string' && !!value.trim() && value.length <= 120;

export function validatePortalLayout(value: unknown, moduleIds: string[]): string[] {
  const layout = record(value);
  const errors: string[] = [];
  if (layout.columns !== 12 || !onlyKeys(layout, ['columns', 'widgets']))
    errors.push('layout_shape');
  if (!Array.isArray(layout.widgets)) return [...errors, 'widgets'];
  if (layout.widgets.length > maxWidgets) errors.push('widget_limit');
  const ids: string[] = [];
  for (const value of layout.widgets) {
    const row = record(value);
    if (!onlyKeys(row, ['id', 'type', 'title', 'module_id', 'width', 'height']))
      errors.push('widget_shape');
    if (typeof row.id !== 'string' || !row.id.trim() || row.id.length > 80 || ids.includes(row.id))
      errors.push('widget_id');
    else ids.push(row.id);
    if (!widgetTypes.some((type) => type === row.type)) errors.push('widget_type');
    if (!validTitle(row.title)) errors.push('widget_title');
    if (typeof row.module_id !== 'string' || !moduleIds.includes(row.module_id))
      errors.push('widget_module');
    if (!isIntegerBetween(row.width, 1, 12) || !isIntegerBetween(row.height, 1, 8))
      errors.push('widget_size');
  }
  return [...new Set(errors)];
}
export function validatePortalTheme(value: unknown): string[] {
  const theme = record(value);
  const errors: string[] = [];
  if (!onlyKeys(theme, ['primary_color', 'background_color', 'title'])) errors.push('theme_shape');
  if (!safeHex(theme.primary_color) || !safeHex(theme.background_color)) errors.push('theme_color');
  if (!validTitle(theme.title)) errors.push('theme_title');
  return errors;
}
export function validateChartConfig(
  value: unknown,
  properties?: Record<string, unknown>,
): string[] {
  const config = record(value);
  const errors: string[] = [];
  if (!onlyKeys(config, ['type', 'title', 'x_field', 'y_field', 'legend']))
    errors.push('chart_shape');
  if (!chartTypes.some((type) => type === config.type)) errors.push('chart_type');
  if (!validTitle(config.title)) errors.push('chart_title');
  if (typeof config.legend !== 'boolean') errors.push('chart_legend');
  if (
    typeof config.x_field !== 'string' ||
    typeof config.y_field !== 'string' ||
    (config.type !== 'table' && (!config.x_field.trim() || !config.y_field.trim()))
  )
    errors.push('chart_fields');
  const dimensions = reportDimensions(properties?.dimensions)
    .map((value) => value.trim())
    .filter(Boolean);
  const measures = reportMeasures(properties?.measures)
    .map((value) => value.name.trim())
    .filter(Boolean);
  if (
    config.type !== 'table' &&
    ((dimensions.length && !dimensions.includes(text(config.x_field))) ||
      (measures.length && !measures.includes(text(config.y_field))))
  )
    errors.push('chart_field_reference');
  return [...new Set(errors)];
}
