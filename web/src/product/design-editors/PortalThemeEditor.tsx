import {
  portalTheme,
  safeHex,
  validatePortalTheme,
  type DesignEditorProps,
  type PortalTheme,
} from './design-models';
import { DesignHeader, DesignWarning } from './design-ui';
import { designField, designInput, designPanel, useDesignText } from './design-text';

export default function PortalThemeEditor({ value, onChange, disabled }: DesignEditorProps) {
  const text = useDesignText();
  const theme = portalTheme(value);
  const update = (patch: Partial<PortalTheme>) => onChange({ ...theme, ...patch });
  const primary = safeHex(theme.primary_color) ? theme.primary_color : '#0D9488';
  const background = safeHex(theme.background_color) ? theme.background_color : '#F4F7FA';
  const errors = value === undefined ? [] : validatePortalTheme(value);
  return (
    <fieldset className={designPanel} disabled={disabled}>
      <legend className="sr-only">{text('门户主题配置', 'Portal theme settings')}</legend>
      <DesignHeader title={text('门户主题配置', 'Portal theme settings')} />
      <div className="grid gap-3 sm:grid-cols-3">
        <label className={designField}>
          <span>{text('门户标题', 'Portal title')}</span>
          <input
            className={designInput}
            value={theme.title}
            required
            maxLength={120}
            onChange={(event) => update({ title: event.target.value })}
          />
        </label>
        <div>
          <label className={designField}>
            <span>{text('主题色', 'Primary color')}</span>
            <input
              className={designInput}
              value={theme.primary_color}
              pattern="#[0-9a-fA-F]{6}"
              maxLength={7}
              required
              aria-invalid={!safeHex(theme.primary_color)}
              onChange={(event) => update({ primary_color: event.target.value })}
            />
          </label>
          <input
            className="mt-2 h-8 w-full cursor-pointer"
            type="color"
            aria-label={text('选择主题色', 'Pick a primary color')}
            value={primary}
            onChange={(event) => update({ primary_color: event.target.value })}
          />
        </div>
        <div>
          <label className={designField}>
            <span>{text('背景色', 'Background color')}</span>
            <input
              className={designInput}
              value={theme.background_color}
              pattern="#[0-9a-fA-F]{6}"
              maxLength={7}
              required
              aria-invalid={!safeHex(theme.background_color)}
              onChange={(event) => update({ background_color: event.target.value })}
            />
          </label>
          <input
            className="mt-2 h-8 w-full cursor-pointer"
            type="color"
            aria-label={text('选择背景色', 'Pick a background color')}
            value={background}
            onChange={(event) => update({ background_color: event.target.value })}
          />
        </div>
      </div>
      <p className="mt-3 text-xs text-[var(--color-text-tertiary)]">
        {text(
          '仅接受 #RRGGBB 格式颜色，不支持 CSS、HTML 或脚本。',
          'Colors must use #RRGGBB format. CSS, HTML and scripts are not supported.',
        )}
      </p>
      {!!errors.length && (
        <DesignWarning>
          {text(
            '请填写门户标题与两项有效的 HEX 颜色。',
            'Provide a portal title and two valid HEX colors.',
          )}
        </DesignWarning>
      )}
      <div
        aria-label={text('门户主题预览', 'Portal theme preview')}
        className="mt-4 rounded-md border border-[var(--color-border-default)] p-4"
        style={{ backgroundColor: background }}
      >
        <p className="text-sm font-medium" style={{ color: primary }}>
          {theme.title || text('未设置门户标题', 'Portal title not configured')}
        </p>
        <span
          className="mt-3 inline-flex rounded-md px-3 py-2 text-xs text-white"
          style={{ backgroundColor: primary }}
        >
          {text('操作样式预览', 'Action style preview')}
        </span>
        <p className="mt-2 text-xs text-slate-600">
          {text(
            '仅预览主题，不展示业务结果。',
            'Theme preview only; no business results are shown.',
          )}
        </p>
      </div>
    </fieldset>
  );
}
