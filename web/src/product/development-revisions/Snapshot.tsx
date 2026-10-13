import { useTranslation } from 'react-i18next';
import type { Revision } from './types';
const object = (value: unknown): Record<string, unknown> =>
  value && typeof value === 'object' && !Array.isArray(value)
    ? Object.fromEntries(Object.entries(value))
    : {};
const rows = (value: unknown) => (Array.isArray(value) ? value.map(object) : []);
const textValue = (value: unknown) =>
  typeof value === 'string' ? value : JSON.stringify(value ?? null);
export default function RevisionSnapshot({ revision }: { revision: Revision }) {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const resource = revision.snapshot,
    definition = object(resource.properties.definition);
  const parameters = rows(
    resource.kind === 'sql_document' ? resource.properties.parameters : definition.parameters,
  );
  const nodes = rows(definition.nodes);
  return (
    <section className="data-panel p-4 space-y-4">
      <h3 className="font-semibold">
        {text('不可变修订详情', 'Immutable revision details')} · v{revision.source_version}
      </h3>
      <dl className="grid gap-2 text-sm">
        <div>
          {resource.code} · {resource.name}
        </div>
        <div>
          {text('保存者', 'Saved by')}: {revision.saved_by} · {revision.saved_at}
        </div>
        <div>
          {text('保存原因', 'Reason')}: {revision.reason}
        </div>
        <div className="break-all">SHA256: {revision.snapshot_hash}</div>
        <div>
          {text('冻结时状态', 'Status at save')}: {resource.status}
        </div>
        {resource.description && (
          <div>
            {text('说明', 'Description')}: {resource.description}
          </div>
        )}
        {resource.kind === 'development_workflow' && (
          <>
            <div>
              {text('周期配置', 'Schedule definition')}:{' '}
              {textValue(resource.properties.cron_expression)}
            </div>
            <div>
              {text('时区', 'Timezone')}: {textValue(resource.properties.timezone)}
            </div>
            {'retry_limit' in resource.properties && (
              <div>
                {text('重试次数配置', 'Retry configuration')}:{' '}
                {textValue(resource.properties.retry_limit)}
              </div>
            )}
          </>
        )}
      </dl>
      {resource.kind === 'sql_document' && (
        <pre className="rounded bg-[var(--color-bg-secondary)] p-3 whitespace-pre-wrap break-all">
          {textValue(resource.properties.sql)}
        </pre>
      )}
      {nodes.map((node, index) => (
        <article
          key={index}
          className="rounded border border-[var(--color-border-default)] p-3 space-y-2"
        >
          <h4>
            {textValue(node.id)} · {textValue(node.name)}
          </h4>
          <p className="text-xs">
            {text('依赖节点', 'Depends on')}: {textValue(node.depends_on)}
          </p>
          <pre className="whitespace-pre-wrap break-all">{textValue(node.sql)}</pre>
        </article>
      ))}
      <h4>{text('类型化参数定义', 'Typed parameter definitions')}</h4>
      {parameters.length ? (
        <table className="w-full text-sm">
          <thead>
            <tr>
              <th>{text('名称', 'Name')}</th>
              <th>{text('类型', 'Type')}</th>
              <th>{text('必填', 'Required')}</th>
              <th>{text('默认值', 'Default')}</th>
            </tr>
          </thead>
          <tbody>
            {parameters.map((parameter, index) => (
              <tr key={index}>
                <td>{textValue(parameter.name)}</td>
                <td>{textValue(parameter.data_type)}</td>
                <td>{parameter.required === true ? text('是', 'Yes') : text('否', 'No')}</td>
                <td>{'default_value' in parameter ? textValue(parameter.default_value) : '—'}</td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : (
        <p className="text-sm">
          {text('该修订未声明参数', 'This revision declares no parameters')}
        </p>
      )}
      {resource.references.length > 0 && (
        <div>
          <h4>{text('冻结定义引用', 'Frozen definition references')}</h4>
          <ul>
            {resource.references.map((ref) => (
              <li
                className="break-all text-sm"
                key={`${ref.field}:${ref.target_kind}:${ref.target_id}`}
              >
                {ref.field}: {ref.target_kind} / {ref.target_id}
              </li>
            ))}
          </ul>
        </div>
      )}
    </section>
  );
}
