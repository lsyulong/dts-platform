import { lazy, Suspense, useState } from 'react';
import { FileCode2, FileText, LockKeyhole } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import StructuredPropertyEditor, {
  hasStructuredPropertyEditor,
} from '../editors/StructuredPropertyEditor';
import type { ProductResource } from '../resource-types';
import type { ProductWorkspaceProps } from '../workspace-types';
import { localized } from '../ui-text';
import RecordDirectory from './RecordDirectory';
import { editorWorkspaceText } from './editor-workspace-text';
import { FieldValue, WorkspaceActions, WorkspaceStatus } from './workspace-ui';
import './editor-workspaces.css';

const codeFields = new Set([
  'sql',
  'ddl',
  'expression',
  'body',
  'template',
  'content',
  'subject_template',
  'body_template',
]);
const ignoreReadOnlyChange = () => undefined;
const ModelStandardBindingsPanel = lazy(
  () => import('../../pages/product/ModelStandardBindingsPanel'),
);
const TagAssignmentsPanel = lazy(() => import('../TagAssignmentsPanel'));
const AlertPolicyEvaluationsPanel = lazy(() => import('../AlertPolicyEvaluationsPanel'));

export function RecordMetadata({
  record,
  language,
}: {
  record: ProductResource;
  language: string;
}) {
  const text = editorWorkspaceText(language);
  const parsed = new Date(record.updated_at);
  const modified = Number.isNaN(parsed.getTime())
    ? record.updated_at
    : parsed.toLocaleString(language);
  return (
    <dl className="product-definition-metadata">
      <div>
        <dt>{text.code}</dt>
        <dd>
          <code>{record.code}</code>
        </dd>
      </div>
      <div>
        <dt>{text.version}</dt>
        <dd>{record.version}</dd>
      </div>
      <div>
        <dt>{text.status}</dt>
        <dd>
          <WorkspaceStatus status={record.status} />
        </dd>
      </div>
      <div>
        <dt>{text.modified}</dt>
        <dd>{modified}</dd>
      </div>
    </dl>
  );
}

export default function EditorWorkspace(props: ProductWorkspaceProps) {
  const { i18n } = useTranslation();
  const text = editorWorkspaceText(i18n.language);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const record = props.records.find((item) => item.id === selectedId) ?? props.records[0];

  return (
    <div className="product-editor-workspace">
      <RecordDirectory
        records={props.records}
        selectedId={record?.id}
        onSelect={(item) => setSelectedId(item.id)}
      />
      <section
        className="product-definition-panel"
        aria-label={localized(props.module.title, i18n.language)}
      >
        {!record ? (
          <div className="product-workspace-empty">
            <FileText size={32} aria-hidden="true" />
            <h3>{text.noRecords}</h3>
            <p>{text.noRecordsHelp}</p>
          </div>
        ) : (
          <>
            <header className="product-definition-header">
              <div>
                <span className="product-workspace-eyebrow">
                  {localized(props.module.title, i18n.language)}
                </span>
                <h3>{record.name}</h3>
              </div>
              <div className="product-definition-controls">
                {!props.canWrite && (
                  <span className="product-workspace-readonly">
                    <LockKeyhole size={13} aria-hidden="true" />
                    {text.readonly}
                  </span>
                )}
                <WorkspaceActions
                  {...props}
                  record={record}
                  excludeActions={record.kind === 'tag' ? ['unassign'] : []}
                />
              </div>
            </header>
            <RecordMetadata record={record} language={i18n.language} />
            {record.description && (
              <p className="product-definition-description">{record.description}</p>
            )}
            <div className="product-definition-fields">
              {!props.schema.fields.length && (
                <p className="product-workspace-muted">{text.noProperties}</p>
              )}
              {props.schema.fields.map((field) => {
                const value = record.properties[field.key];
                const isCode = codeFields.has(field.key) && typeof value === 'string';
                return (
                  <section
                    key={field.key}
                    className={
                      isCode
                        ? 'product-definition-field product-code-field'
                        : 'product-definition-field'
                    }
                  >
                    <h4>
                      {isCode && <FileCode2 size={15} aria-hidden="true" />}
                      {localized(field.label, i18n.language)}
                    </h4>
                    {value === undefined || value === null ? (
                      <p className="product-workspace-muted">{text.notConfigured}</p>
                    ) : hasStructuredPropertyEditor(props.schema.kind, field.key) ? (
                      <StructuredPropertyEditor
                        kind={props.schema.kind}
                        fieldKey={field.key}
                        value={value}
                        disabled
                        onChange={ignoreReadOnlyChange}
                      />
                    ) : isCode ? (
                      <>
                        <pre className="product-code-preview">
                          <code>{value}</code>
                        </pre>
                        <p className="product-workspace-muted">{text.sqlReadonly}</p>
                      </>
                    ) : typeof value === 'object' ? (
                      <pre className="product-json-preview">{JSON.stringify(value, null, 2)}</pre>
                    ) : (
                      <div className="product-definition-value">
                        <FieldValue field={field} record={record} language={i18n.language} />
                      </div>
                    )}
                  </section>
                );
              })}
            </div>
            {record.kind === 'data_model' && (
              <Suspense fallback={<p role="status">…</p>}>
                <ModelStandardBindingsPanel model={record} />
              </Suspense>
            )}
            {record.kind === 'tag' && (
              <Suspense fallback={<p role="status">…</p>}>
                <TagAssignmentsPanel
                  record={record}
                  busy={props.busy}
                  canWrite={props.canWrite}
                  onUnassign={(targetKind, targetId) =>
                    props.onAction(record, 'unassign', {
                      target_kind: targetKind,
                      target_id: targetId,
                    })
                  }
                />
              </Suspense>
            )}
            {record.kind === 'alert_policy' && (
              <Suspense fallback={<p role="status">…</p>}>
                <AlertPolicyEvaluationsPanel record={record} />
              </Suspense>
            )}
          </>
        )}
      </section>
    </div>
  );
}
