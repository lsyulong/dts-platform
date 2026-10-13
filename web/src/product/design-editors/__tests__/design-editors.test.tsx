import { useState } from 'react';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import DesignPropertyEditor, { hasDesignPropertyEditor } from '../DesignPropertyEditor';
import {
  boundedInteger,
  createPortalWidget,
  moveWidget,
  safeHex,
  validateChartConfig,
  validatePortalLayout,
  validatePortalTheme,
  type PortalLayout,
} from '../design-models';

function Harness({
  kind,
  fieldKey,
  initial,
  properties,
}: {
  kind: string;
  fieldKey: string;
  initial?: unknown;
  properties?: Record<string, unknown>;
}) {
  const [value, setValue] = useState<unknown>(initial);
  return (
    <>
      <DesignPropertyEditor
        kind={kind}
        fieldKey={fieldKey}
        value={value}
        onChange={setValue}
        properties={properties}
      />
      <output data-testid="value">{JSON.stringify(value)}</output>
    </>
  );
}

const first = {
  id: 'widget-1',
  type: 'task_list' as const,
  title: 'Task overview',
  module_id: 'integration-tasks',
  width: 6,
  height: 2,
};
const layout: PortalLayout = {
  columns: 12,
  widgets: [first, { ...first, id: 'widget-2', title: 'Second widget' }],
};

describe('Design property contracts', () => {
  it('rejects malformed layouts, unknown modules, duplicate IDs and out-of-range dimensions', () => {
    expect(validatePortalLayout(layout, ['integration-tasks'])).toEqual([]);
    expect(
      validatePortalLayout(
        {
          columns: 10,
          widgets: [
            { ...first, width: 13, height: 0, module_id: 'https://example.invalid' },
            first,
          ],
        },
        ['integration-tasks'],
      ),
    ).toEqual(
      expect.arrayContaining(['layout_shape', 'widget_size', 'widget_module', 'widget_id']),
    );
    expect(boundedInteger(99, 1, 12)).toBe(12);
    expect(boundedInteger(-8, 1, 8)).toBe(1);
    expect(boundedInteger(Number.NaN, 1, 12, 6)).toBe(6);
  });

  it('moves stable component IDs without mutating the source and allocates a unique next ID', () => {
    expect(moveWidget(layout, 0, 1).widgets.map((widget) => widget.id)).toEqual([
      'widget-2',
      'widget-1',
    ]);
    expect(layout.widgets.map((widget) => widget.id)).toEqual(['widget-1', 'widget-2']);
    expect(createPortalWidget(layout.widgets).id).toBe('widget-3');
    expect(moveWidget(layout, 0, -1)).toBe(layout);
  });

  it('accepts only full HEX colors and refuses extra style or script properties', () => {
    expect(safeHex('#0D9488')).toBe(true);
    expect(safeHex('url(javascript:alert(1))')).toBe(false);
    expect(safeHex('#fff')).toBe(false);
    expect(
      validatePortalTheme({
        primary_color: '#0D9488',
        background_color: '#ffffff',
        title: 'Portal',
      }),
    ).toEqual([]);
    expect(
      validatePortalTheme({
        primary_color: 'red;background:url(https://example.invalid)',
        background_color: '#ffffff',
        title: 'Portal',
        css: 'body{}',
      }),
    ).toEqual(expect.arrayContaining(['theme_color', 'theme_shape']));
  });

  it('requires axes for actual charts and checks declared field references', () => {
    const config = { type: 'bar', title: 'Report', x_field: '', y_field: '', legend: true };
    expect(validateChartConfig(config)).toContain('chart_fields');
    expect(validateChartConfig({ ...config, type: 'table' })).toEqual([]);
    expect(
      validateChartConfig(
        { ...config, x_field: 'missing', y_field: 'count' },
        {
          dimensions: ['department'],
          measures: [{ name: 'count', expression: 'employee_id', aggregate: 'count' }],
        },
      ),
    ).toContain('chart_field_reference');
  });

  it('removes only the selected portal widget and preserves the remaining layout', async () => {
    render(<Harness kind="portal" fieldKey="layout" initial={layout} />);
    await userEvent.setup().click(screen.getByRole('button', { name: '删除组件 1' }));
    expect(JSON.parse(screen.getByTestId('value').textContent || 'null')).toEqual({
      columns: 12,
      widgets: [layout.widgets[1]],
    });
    expect(screen.getByText('布局预览')).toBeInTheDocument();
    expect(screen.queryByText('100%')).not.toBeInTheDocument();
  });

  it('never applies an unsafe theme string to preview styles', () => {
    render(
      <Harness
        kind="portal"
        fieldKey="theme"
        initial={{
          primary_color: 'url(javascript:alert(1))',
          background_color: '#ffffff',
          title: '<script>bad()</script>',
        }}
      />,
    );
    const preview = screen.getByLabelText('门户主题预览');
    expect(preview.innerHTML).not.toContain('<script>');
    expect(preview.innerHTML).not.toContain('javascript:');
    expect(screen.getByRole('alert')).toHaveTextContent('有效的 HEX');
  });

  it('uses current report declarations for chart field selectors without invented rows', () => {
    render(
      <Harness
        kind="bi_report"
        fieldKey="chart_config"
        initial={{ type: 'table', title: 'Report', x_field: '', y_field: '', legend: false }}
        properties={{
          dimensions: ['department'],
          measures: [{ name: 'employee_count', expression: 'employee_id', aggregate: 'count' }],
        }}
      />,
    );
    expect(screen.getByRole('combobox', { name: '分类 / X 轴字段' })).toContainHTML('department');
    expect(screen.getByRole('combobox', { name: '度量 / Y 轴字段' })).toContainHTML(
      'employee_count',
    );
    expect(screen.getByText('执行真实查询后显示结果行')).toBeInTheDocument();
  });

  it('outputs SQL data_type in API schema and dispatches only matching business properties', async () => {
    render(<Harness kind="data_api" fieldKey="response_schema" initial={[]} />);
    const interactions = userEvent.setup();
    await interactions.click(screen.getByRole('button', { name: '新增字段' }));
    await interactions.type(screen.getByLabelText('字段名称 1'), 'employee_count');
    expect(JSON.parse(screen.getByTestId('value').textContent || 'null')).toEqual([
      { name: 'employee_count', data_type: 'VARCHAR', nullable: true, description: '' },
    ]);
    expect(hasDesignPropertyEditor('data_api', 'parameters')).toBe(false);
    expect(hasDesignPropertyEditor('portal', 'authorized_role_ids')).toBe(false);
    expect(hasDesignPropertyEditor('metadata_object', 'columns')).toBe(false);
    expect(hasDesignPropertyEditor('bi_report', 'measures')).toBe(true);
  });
});
