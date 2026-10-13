import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button, ErrorBanner } from '../../components/ui';
import { useProductResources } from '../../product/api';
import type { ProductResource } from '../../product/resource-types';
import { useAuthStore } from '../../stores/auth';
import { apiErrorMessage } from '../../lib/apiError';
import { captureSession, isCurrentSession } from '../../lib/session';
import StandardRevisionSelector from './StandardRevisionSelector';
import {
  standardIdentity,
  useModelStandardBindings,
  useStandardOperation,
  type ModelStandardsRequest,
  type StandardInspectionResult,
  type StandardVersionRequest,
} from './standards-api';

const input =
  'mt-2 block w-full rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] px-3 py-2 text-sm';
type FieldBinding = {
  column_name: string;
  field_standard: StandardVersionRequest | null;
  naming_standard: StandardVersionRequest | null;
};
function columnNames(model: ProductResource) {
  return Array.isArray(model.properties.columns)
    ? model.properties.columns.flatMap((c: unknown) =>
        typeof c === 'object' && c !== null && 'name' in c && typeof c.name === 'string'
          ? [c.name]
          : [],
      )
    : [];
}
export default function ModelStandardBindingsPanel({
  model,
  onChanged,
}: {
  model: ProductResource;
  onChanged?: (model: ProductResource) => void;
}) {
  const identity = useAuthStore((s) =>
    JSON.stringify([s.user?.tenant_id, s.user?.id, s.token, s.user?.permissions]),
  );
  return (
    <BindingPanel
      key={`${identity}:${model.id}:${model.version}`}
      model={model}
      onChanged={onChanged}
    />
  );
}
function BindingPanel({
  model,
  onChanged,
}: {
  model: ProductResource;
  onChanged?: (model: ProductResource) => void;
}) {
  const { i18n } = useTranslation(),
    text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const user = useAuthStore((s) => s.user),
    canRead =
      !!user?.permissions.includes('models:read') &&
      !!user.permissions.includes('standards:read') &&
      model.tenant_id === user.tenant_id;
  const canWrite = canRead && !!user?.permissions.includes('models:write'),
    canInspect =
      canRead &&
      !!user?.permissions.includes('standards:execute') &&
      !!user.permissions.includes('metadata:read');
  const query = useModelStandardBindings(model.id, canRead),
    operation = useStandardOperation();
  const [fields, setFields] = useState<FieldBinding[]>(() =>
      columnNames(model).map((column_name) => ({
        column_name,
        field_standard: null,
        naming_standard: null,
      })),
    ),
    [coverage, setCoverage] = useState<ModelStandardsRequest['coverage']>('bound_fields'),
    [table, setTable] = useState<StandardVersionRequest | null>(null),
    [upgrade, setUpgrade] = useState(false),
    [error, setError] = useState(''),
    [notice, setNotice] = useState(''),
    [inspection, setInspection] = useState<StandardInspectionResult | null>(null);
  const [metadataId, setMetadataId] = useState(''),
    [metadataOffset, setMetadataOffset] = useState(0);
  const metadata = useProductResources(
    'metadata_object',
    { limit: 20, offset: metadataOffset },
    canInspect,
  );
  const objects = !metadata.error
    ? (metadata.data?.data ?? []).filter(
        (r) => r.tenant_id === user?.tenant_id && r.kind === 'metadata_object',
      )
    : [];
  const selectedMetadata = objects.find((r) => r.id === metadataId);
  useEffect(() => {
    if (query.error || query.data === undefined) return;
    const saved = query.data;
    setFields(
      columnNames(model).map((column_name) => {
        const c = saved?.columns.find((c) => c.column_name === column_name);
        return {
          column_name,
          field_standard: c?.field_standard
            ? { id: c.field_standard.id, version: c.field_standard.revision_version }
            : null,
          naming_standard: c?.naming_standard
            ? { id: c.naming_standard.id, version: c.naming_standard.revision_version }
            : null,
        };
      }),
    );
    setCoverage(saved?.coverage ?? 'bound_fields');
    setTable(
      saved?.table_naming
        ? { id: saved.table_naming.id, version: saved.table_naming.revision_version }
        : null,
    );
  }, [query.data, query.error, model]);
  const sourceVisible = canRead && !query.error && !query.isPending;
  const setField = (
    index: number,
    key: 'field_standard' | 'naming_standard',
    value: StandardVersionRequest | null,
  ) => {
    setFields(fields.map((c, i) => (i === index ? { ...c, [key]: value } : c)));
    setInspection(null);
    setNotice('');
  };
  const complete =
    fields.every((c) => [c.field_standard, c.naming_standard].every((r) => !r || r.version > 0)) &&
    (!table || table.version > 0) &&
    (coverage !== 'all_fields' || fields.every((c) => !!c.field_standard));
  const configured = fields.some((c) => c.field_standard || c.naming_standard) || !!table;
  const save = async () => {
    if (
      !canWrite ||
      !sourceVisible ||
      !complete ||
      !configured ||
      operation.isPending ||
      (model.status === 'published' && !upgrade)
    )
      return;
    const session = captureSession(),
      identity = standardIdentity();
    setError('');
    setNotice('');
    setInspection(null);
    try {
      const result = await operation.mutateAsync({
        type: 'bind',
        modelId: model.id,
        request: {
          expected_model_version: model.version,
          expected_binding_version: query.data?.binding_version ?? null,
          coverage,
          columns: fields.filter((c) => c.field_standard || c.naming_standard),
          table_naming: table,
          upgrade,
        },
      });
      if (
        isCurrentSession(session) &&
        identity === standardIdentity() &&
        result.type !== 'inspect'
      ) {
        setNotice(
          text(
            '真实标准版本已绑定。模型仍需受控物理发布。',
            'The published standard revisions are bound. The model still requires controlled physical publication.',
          ),
        );
        onChanged?.(result.resource);
      }
    } catch (e) {
      if (isCurrentSession(session) && identity === standardIdentity())
        setError(
          apiErrorMessage(
            e,
            text('绑定失败；未声明标准通过', 'Binding failed. Standards were not declared passed.'),
          ),
        );
    }
  };
  const inspect = async () => {
    if (
      !sourceVisible ||
      !canInspect ||
      !query.data ||
      !selectedMetadata ||
      metadata.error ||
      operation.isPending
    )
      return;
    const session = captureSession(),
      identity = standardIdentity();
    setError('');
    setInspection(null);
    try {
      const result = await operation.mutateAsync({
        type: 'inspect',
        modelId: model.id,
        request: {
          expected_model_version: model.version,
          expected_binding_version: query.data.binding_version,
          metadata_id: selectedMetadata.id,
          metadata_version: selectedMetadata.version,
        },
      });
      if (isCurrentSession(session) && identity === standardIdentity() && result.type === 'inspect')
        setInspection(result.result);
    } catch (e) {
      if (isCurrentSession(session) && identity === standardIdentity())
        setError(
          apiErrorMessage(
            e,
            text(
              '真实结构检查失败或源证明已失效',
              'Actual structure inspection failed or source evidence is no longer valid',
            ),
          ),
        );
    }
  };
  if (!canRead)
    return (
      <ErrorBanner
        message={text(
          '当前用户缺少模型或标准查看权限',
          'Your account cannot read this model or its standards',
        )}
      />
    );
  return (
    <section className="space-y-5 rounded-lg border border-[var(--color-border-default)] bg-[var(--color-bg-primary)] p-5">
      <div className="flex items-start justify-between gap-3">
        <div>
          <h2 className="text-lg font-semibold">
            {text('模型字段标准与版本绑定', 'Model field standards and pinned revisions')}
          </h2>
          <p className="mt-2 text-sm text-[var(--color-text-secondary)]">
            {text(
              '仅可引用真实已发布且未撤回的版本。定义验证不等于数据内容合规。',
              'Only real active published revisions can be bound. Definition validation does not establish data-value compliance.',
            )}
          </p>
        </div>
        <Button
          variant="outline"
          disabled={query.isFetching}
          onClick={() => {
            setInspection(null);
            void query.refetch();
          }}
        >
          {text('刷新绑定', 'Refresh bindings')}
        </Button>
      </div>
      {(error || query.error) && (
        <div role="alert">
          <ErrorBanner
            message={
              error ||
              apiErrorMessage(
                query.error,
                text('真实绑定读取失败', 'Could not read actual bindings'),
              )
            }
          />
        </div>
      )}
      {query.isPending && (
        <p role="status">{text('读取真实标准绑定…', 'Reading actual standard bindings…')}</p>
      )}
      {notice && sourceVisible && (
        <p role="status" className="text-sm text-[var(--color-accent)]">
          {notice}
        </p>
      )}
      {sourceVisible && (
        <>
          <p className="text-sm">
            {query.data
              ? `${text('实际绑定版本', 'Actual binding version')}: ${query.data.binding_version}`
              : text(
                  '未配置标准。模型发布回执将明确 not_configured，不能宣称标准校验通过。',
                  'Standards are not configured. A model publication receipt will explicitly state not_configured, not that standards passed.',
                )}
          </p>
          <label className="block text-sm">
            {text('标准覆盖范围', 'Standard coverage')}
            <select
              aria-label={text('标准覆盖范围', 'Standard coverage')}
              className={input}
              disabled={!canWrite || operation.isPending}
              value={coverage}
              onChange={(e) =>
                setCoverage(e.target.value === 'all_fields' ? 'all_fields' : 'bound_fields')
              }
            >
              <option value="bound_fields">
                {text('仅显式绑定字段', 'Only explicitly bound fields')}
              </option>
              <option value="all_fields">
                {text('每个字段必须绑定字段标准', 'Every field requires a field standard')}
              </option>
            </select>
          </label>
          <StandardRevisionSelector
            kind="naming_standard"
            objectType="table"
            label={text('表命名标准', 'Table naming standard')}
            value={table}
            onChange={setTable}
            disabled={!canWrite || operation.isPending}
          />
          <div className="space-y-4">
            {fields.map((field, index) => (
              <fieldset
                key={field.column_name}
                className="rounded-md border border-[var(--color-border-default)] p-4"
              >
                <legend className="px-2 text-sm font-semibold">{field.column_name}</legend>
                <div className="grid gap-4 lg:grid-cols-2">
                  <StandardRevisionSelector
                    kind="field_standard"
                    label={`${field.column_name} ${text('字段标准', 'field standard')}`}
                    value={field.field_standard}
                    onChange={(v) => setField(index, 'field_standard', v)}
                    disabled={!canWrite || operation.isPending}
                  />
                  <StandardRevisionSelector
                    kind="naming_standard"
                    objectType="column"
                    label={`${field.column_name} ${text('命名标准', 'naming standard')}`}
                    value={field.naming_standard}
                    onChange={(v) => setField(index, 'naming_standard', v)}
                    disabled={!canWrite || operation.isPending}
                  />
                </div>
              </fieldset>
            ))}
          </div>
          {model.status === 'published' && (
            <label className="flex gap-2 text-sm">
              <input
                type="checkbox"
                checked={upgrade}
                disabled={!canWrite || operation.isPending}
                onChange={(e) => setUpgrade(e.target.checked)}
              />
              {text(
                '显式升级绑定：将已发布模型回到草稿，再核对物理结构并重新发布。',
                'Explicitly upgrade bindings: return the published model to draft, then reconcile the actual structure and publish again.',
              )}
            </label>
          )}
          <Button
            disabled={
              !canWrite ||
              operation.isPending ||
              !complete ||
              !configured ||
              (model.status === 'published' && !upgrade)
            }
            onClick={() => void save()}
          >
            {text('保存真实版本绑定', 'Save actual revision bindings')}
          </Button>
          <div className="space-y-3 border-t border-[var(--color-border-default)] pt-5">
            <h3 className="font-semibold">
              {text('已采集物理结构检查', 'Collected physical structure inspection')}
            </h3>
            <p className="text-sm text-[var(--color-text-secondary)]">
              {text(
                '只核对实际采集快照的库、schema、表、字段类型和可空属性；不扫描枚举数据值或验证计量语义。快照观测时间不代表当前实时状态。',
                'Checks only the catalog, schema, table, field types and nullability of a real collected snapshot. Enum data values and unit semantics are not scanned. Snapshot observation time does not establish live state.',
              )}
            </p>
            {canInspect ? (
              <>
                <label className="block text-sm">
                  {text('实际采集元数据对象', 'Actual collected metadata object')}
                  <select
                    aria-label={text('实际采集元数据对象', 'Actual collected metadata object')}
                    value={metadataId}
                    onChange={(e) => {
                      setMetadataId(e.target.value);
                      setInspection(null);
                    }}
                    className={input}
                    disabled={operation.isPending || !!metadata.error}
                  >
                    <option value="">
                      {text(
                        '选择实际对象，服务端核验三段身份与内部证明',
                        'Select an object; the server verifies its three-part identity and internal evidence',
                      )}
                    </option>
                    {objects.map((r) => (
                      <option key={r.id} value={r.id}>
                        {r.name} · v{r.version}
                      </option>
                    ))}
                  </select>
                </label>
                <div className="flex gap-2">
                  <Button
                    variant="outline"
                    disabled={!metadataOffset || metadata.isFetching}
                    onClick={() => {
                      setMetadataOffset(Math.max(0, metadataOffset - 20));
                      setMetadataId('');
                      setInspection(null);
                    }}
                  >
                    {text('上一页对象', 'Previous objects')}
                  </Button>
                  <Button
                    variant="outline"
                    disabled={
                      !!metadata.error ||
                      metadataOffset + 20 >= (metadata.data?.total ?? 0) ||
                      metadata.isFetching
                    }
                    onClick={() => {
                      setMetadataOffset(metadataOffset + 20);
                      setMetadataId('');
                      setInspection(null);
                    }}
                  >
                    {text('下一页对象', 'Next objects')}
                  </Button>
                  <Button
                    disabled={
                      !query.data ||
                      !selectedMetadata ||
                      !!metadata.error ||
                      operation.isPending ||
                      model.status !== 'published'
                    }
                    onClick={() => void inspect()}
                  >
                    {text('检查实际采集结构', 'Inspect actual collected structure')}
                  </Button>
                </div>
              </>
            ) : (
              <p>
                {text(
                  '需要标准执行及元数据查看权限才能检查实际结构。',
                  'Standard execution and metadata read permissions are required to inspect actual structure.',
                )}
              </p>
            )}
            {metadata.error && (
              <ErrorBanner
                message={apiErrorMessage(
                  metadata.error,
                  text(
                    '元数据来源读取失败；状态未知',
                    'Metadata source read failed; state is unknown',
                  ),
                )}
              />
            )}
            {inspection && !metadata.error && (
              <div
                role="status"
                className="rounded-md border border-[var(--color-border-default)] p-4"
              >
                <p className="font-semibold">
                  {inspection.state === 'conformant'
                    ? text(
                        '采集结构符合固定标准',
                        'Collected structure conforms to pinned standards',
                      )
                    : text('采集结构存在差异', 'Collected structure has differences')}
                </p>
                <p className="mt-2 text-xs">
                  {text('检查范围', 'Evaluation scope')}: collected_schema ·{' '}
                  {text('观测时间', 'Observed')}:{' '}
                  {new Date(inspection.metadata_observed_at).toLocaleString(i18n.language)}
                </p>
                <p className="mt-2 text-xs break-all">
                  {text('源指纹', 'Source fingerprint')}: {inspection.source_fingerprint}
                </p>
                {inspection.issues.map((issue, index) => (
                  <p key={index} className="mt-2 text-sm">
                    {issue.column_name ?? text('表', 'Table')}: {issue.message} ({issue.code})
                  </p>
                ))}
              </div>
            )}
          </div>
        </>
      )}
    </section>
  );
}
