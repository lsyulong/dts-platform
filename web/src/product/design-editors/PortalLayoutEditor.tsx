import { ArrowDown, ArrowUp, LayoutGrid, Trash2 } from 'lucide-react';
import { Button } from '../../components/ui/button';
import { productModules } from '../catalog';
import {
  boundedInteger,
  createPortalWidget,
  maxWidgets,
  moveWidget,
  portalLayout,
  widgetTypes,
  validatePortalLayout,
  type DesignEditorProps,
  type PortalWidget,
  type WidgetType,
} from './design-models';
import { DesignHeader, DesignWarning } from './design-ui';
import { designField, designInput, designPanel, useDesignText } from './design-text';

export default function PortalLayoutEditor({ value, onChange, disabled }: DesignEditorProps) {
  const text = useDesignText();
  const layout = portalLayout(value);
  const typeLabel = (type: WidgetType) =>
    ({
      metric: text('指标卡片', 'Metric card'),
      task_list: text('任务列表', 'Task list'),
      datasource_list: text('数据源列表', 'Datasource list'),
      announcement: text('公告', 'Announcement'),
      chart: text('图表', 'Chart'),
      shortcut: text('快捷入口', 'Shortcut'),
    })[type];
  const update = (index: number, patch: Partial<PortalWidget>) =>
    onChange({
      columns: 12,
      widgets: layout.widgets.map((widget, position) =>
        position === index ? { ...widget, ...patch } : widget,
      ),
    });
  const errors =
    value === undefined
      ? []
      : validatePortalLayout(
          value,
          productModules.map((module) => module.id),
        );

  return (
    <fieldset className={designPanel} disabled={disabled}>
      <legend className="sr-only">{text('门户布局设计', 'Portal layout design')}</legend>
      <DesignHeader
        title={text('门户布局设计', 'Portal layout design')}
        button={text('添加组件', 'Add widget')}
        disabled={disabled || layout.widgets.length >= maxWidgets}
        onAdd={() =>
          onChange({
            columns: 12,
            widgets: [...layout.widgets, createPortalWidget(layout.widgets)],
          })
        }
      />
      <p className="mb-4 text-xs leading-5 text-[var(--color-text-tertiary)]">
        {text(
          '固定 12 列栅格，最多 48 个组件。选择模块仅保存数据入口配置；预览不执行查询或展示示例指标。',
          'Use a fixed 12-column grid with up to 48 widgets. Module selection saves configuration only; previews do not execute queries or show sample metrics.',
        )}
      </p>
      <div className="space-y-3">
        {layout.widgets.map((widget, index) => (
          <div
            key={`${widget.id}:${index}`}
            className="rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-3"
          >
            <div className="mb-3 flex items-center justify-between gap-2">
              <span className="break-all text-xs text-[var(--color-text-tertiary)]">
                {widget.id || text('缺少组件 ID', 'Missing widget ID')}
              </span>
              <div className="flex gap-1">
                <Button
                  type="button"
                  size="icon"
                  variant="ghost"
                  disabled={disabled || index === 0}
                  aria-label={`${text('上移组件', 'Move widget up')} ${index + 1}`}
                  onClick={() => onChange(moveWidget(layout, index, -1))}
                >
                  <ArrowUp />
                </Button>
                <Button
                  type="button"
                  size="icon"
                  variant="ghost"
                  disabled={disabled || index === layout.widgets.length - 1}
                  aria-label={`${text('下移组件', 'Move widget down')} ${index + 1}`}
                  onClick={() => onChange(moveWidget(layout, index, 1))}
                >
                  <ArrowDown />
                </Button>
                <Button
                  type="button"
                  size="icon"
                  variant="ghost"
                  aria-label={`${text('删除组件', 'Remove widget')} ${index + 1}`}
                  onClick={() =>
                    onChange({
                      columns: 12,
                      widgets: layout.widgets.filter((_, position) => position !== index),
                    })
                  }
                >
                  <Trash2 />
                </Button>
              </div>
            </div>
            <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
              <label className={designField}>
                <span>
                  {text('组件类型', 'Widget type')} {index + 1}
                </span>
                <select
                  className={designInput}
                  value={widget.type}
                  onChange={(event) => {
                    const type = widgetTypes.find((type) => type === event.target.value);
                    if (type) update(index, { type });
                  }}
                >
                  {widgetTypes.map((type) => (
                    <option key={type} value={type}>
                      {typeLabel(type)}
                    </option>
                  ))}
                </select>
              </label>
              <label className={designField}>
                <span>
                  {text('组件标题', 'Widget title')} {index + 1}
                </span>
                <input
                  className={designInput}
                  value={widget.title}
                  required
                  maxLength={120}
                  onChange={(event) => update(index, { title: event.target.value })}
                />
              </label>
              <label className={`${designField} sm:col-span-2`}>
                <span>
                  {text('关联模块', 'Source module')} {index + 1}
                </span>
                <select
                  className={designInput}
                  value={widget.module_id}
                  required
                  onChange={(event) => update(index, { module_id: event.target.value })}
                >
                  <option value="">{text('请选择模块', 'Choose a module')}</option>
                  {widget.module_id &&
                    !productModules.some((module) => module.id === widget.module_id) && (
                      <option value={widget.module_id}>
                        {text('失效模块', 'Unavailable module')}: {widget.module_id}
                      </option>
                    )}
                  {productModules.map((module) => (
                    <option key={module.id} value={module.id}>
                      {text(module.title.zh, module.title.en)}
                    </option>
                  ))}
                </select>
              </label>
              <label className={designField}>
                <span>
                  {text('宽度（列）', 'Width (columns)')} {index + 1}
                </span>
                <input
                  className={designInput}
                  type="number"
                  min={1}
                  max={12}
                  step={1}
                  value={widget.width}
                  onChange={(event) =>
                    update(index, {
                      width: boundedInteger(Number(event.target.value), 1, 12, widget.width),
                    })
                  }
                />
              </label>
              <label className={designField}>
                <span>
                  {text('高度（行）', 'Height (rows)')} {index + 1}
                </span>
                <input
                  className={designInput}
                  type="number"
                  min={1}
                  max={8}
                  step={1}
                  value={widget.height}
                  onChange={(event) =>
                    update(index, {
                      height: boundedInteger(Number(event.target.value), 1, 8, widget.height),
                    })
                  }
                />
              </label>
            </div>
          </div>
        ))}
        {!layout.widgets.length && (
          <p className="rounded-md border border-dashed border-[var(--color-border-hover)] p-5 text-center text-sm text-[var(--color-text-tertiary)]">
            {text('尚未配置门户组件', 'No portal widgets configured')}
          </p>
        )}
      </div>
      {!!errors.length && (
        <DesignWarning>
          {text(
            '请检查组件标题、模块引用、唯一 ID、尺寸和固定布局格式后保存。',
            'Check widget titles, module references, unique IDs, size limits and the fixed layout schema before saving.',
          )}
        </DesignWarning>
      )}
      <section className="mt-5" aria-label={text('门户布局预览', 'Portal layout preview')}>
        <h3 className="mb-2 flex items-center gap-2 text-sm font-medium text-[var(--color-text-secondary)]">
          <LayoutGrid size={16} />
          {text('布局预览', 'Layout preview')}
        </h3>
        <div className="overflow-x-auto rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-primary)] p-3">
          <div
            className="grid min-w-[540px] gap-2"
            style={{ gridTemplateColumns: 'repeat(12, minmax(0, 1fr))', gridAutoRows: '38px' }}
          >
            {layout.widgets.slice(0, maxWidgets).map((widget, index) => {
              const module = productModules.find((module) => module.id === widget.module_id);
              return (
                <div
                  key={`${widget.id}:${index}`}
                  className="flex min-w-0 flex-col justify-center overflow-hidden rounded-md border border-[var(--color-accent)]/25 bg-[var(--color-accent-subtle)] p-2"
                  style={{ gridColumn: `span ${widget.width}`, gridRow: `span ${widget.height}` }}
                >
                  <p className="truncate text-xs font-medium text-[var(--color-text-primary)]">
                    {widget.title || text('未设置标题', 'Untitled widget')}
                  </p>
                  <span className="truncate text-[10px] text-[var(--color-accent)]">
                    {typeLabel(widget.type)}
                  </span>
                  <span className="truncate text-[10px] text-[var(--color-text-tertiary)]">
                    {module
                      ? text(module.title.zh, module.title.en)
                      : text('待选择模块', 'Select a source module')}
                  </span>
                </div>
              );
            })}
          </div>
        </div>
        <p className="mt-2 text-xs text-[var(--color-text-tertiary)]">
          {text(
            '此处仅预览组件位置与尺寸。实际内容需要发布后的权限校验与业务数据接入。',
            'This previews widget position and size only. Runtime content requires publication, authorization and real data integration.',
          )}
        </p>
      </section>
    </fieldset>
  );
}
