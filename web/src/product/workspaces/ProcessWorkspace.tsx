import { useState } from 'react';
import { ArrowRight, Boxes, GitBranch, LockKeyhole, Workflow } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Link } from 'react-router-dom';
import type { ProductResource, ResourceSchema } from '../resource-types';
import type { ProductWorkspaceProps } from '../workspace-types';
import { localized } from '../ui-text';
import { RecordMetadata } from './EditorWorkspace';
import RecordDirectory from './RecordDirectory';
import { editorWorkspaceText } from './editor-workspace-text';
import { processWorkspaceText } from './process-workspace-text';
import { FieldValue, WorkspaceActions } from './workspace-ui';
import './editor-workspaces.css';
import './process-workspaces.css';

type FlowStep = { keys: string[]; title?: string };

function processSteps(kind: string, language: string): FlowStep[] {
  const text = processWorkspaceText(language);
  switch (kind) {
    case 'realtime_task':
      return [{ keys: ['source_id'] }, { keys: ['mapping'] }, { keys: ['target_id'] }];
    case 'metadata_collection':
      return [
        { keys: ['datasource_id'] },
        { keys: ['schema_filter', 'table_filter'], title: text.collectionScope },
        { keys: ['schedule'], title: text.collectionPlan },
      ];
    case 'master_distribution':
      return [
        { keys: ['model_id'] },
        { keys: ['mapping'] },
        { keys: ['target_id', 'target_table'] },
      ];
    case 'recognition_task':
      return [{ keys: ['object_id'] }, { keys: ['rule_id'] }, { keys: ['sample_limit'] }];
    case 'quality_task':
      return [
        { keys: ['object_id'] },
        { keys: ['rule_ids'] },
        { keys: ['schedule', 'sample_limit'], title: text.executionPolicy },
      ];
    case 'metadata_lineage':
      return [{ keys: ['source_object_id'] }, { keys: ['target_object_id'] }];
    default:
      return [];
  }
}

function PropertyPreview({
  schema,
  record,
  fieldKey,
  language,
}: {
  schema: ResourceSchema;
  record: ProductResource;
  fieldKey: string;
  language: string;
}) {
  const field = schema.fields.find((item) => item.key === fieldKey);
  if (!field) return null;
  const value = record.properties[fieldKey];
  if (value === undefined || value === null || value === '')
    return <p className="product-workspace-muted">{editorWorkspaceText(language).notConfigured}</p>;
  if (typeof value === 'object')
    return <pre className="product-process-value-preview">{JSON.stringify(value, null, 2)}</pre>;
  return (
    <div className="product-process-scalar">
      <FieldValue field={field} record={record} language={language} />
    </div>
  );
}

function ConfigurationFlow({
  record,
  schema,
  language,
}: {
  record: ProductResource;
  schema: ResourceSchema;
  language: string;
}) {
  const text = processWorkspaceText(language);
  const steps = processSteps(schema.kind, language);
  if (!steps.length) return null;
  return (
    <section className="product-process-section" aria-label={text.configurationFlow}>
      <h4>
        <GitBranch size={16} aria-hidden="true" />
        {schema.kind === 'metadata_lineage' ? text.lineage : text.configurationFlow}
      </h4>
      <p className="product-workspace-muted">
        {schema.kind === 'metadata_lineage' ? text.lineageScope : text.flowScope}
      </p>
      <ol className="product-process-steps">
        {steps.map((step, index) => {
          const fields = schema.fields.filter((field) => step.keys.includes(field.key));
          return (
            <li key={step.keys.join(',')}>
              {index > 0 && (
                <ArrowRight className="product-process-arrow" size={17} aria-hidden="true" />
              )}
              <div className="product-process-step">
                <h5>{step.title ?? (fields[0] ? localized(fields[0].label, language) : '—')}</h5>
                {fields.map((field) => (
                  <div key={field.key} className="product-process-step-field">
                    {fields.length > 1 && <span>{localized(field.label, language)}</span>}
                    <PropertyPreview
                      schema={schema}
                      record={record}
                      fieldKey={field.key}
                      language={language}
                    />
                  </div>
                ))}
              </div>
            </li>
          );
        })}
      </ol>
      {schema.kind === 'realtime_task' && (
        <div className="product-process-checkpoint">
          <h5>{text.checkpoint}</h5>
          <PropertyPreview
            schema={schema}
            record={record}
            fieldKey="checkpoint"
            language={language}
          />
        </div>
      )}
      {schema.kind === 'quality_task' && (
        <Link className="product-process-result-link" to="/quality/results">
          {text.qualityResults}
          <ArrowRight size={14} aria-hidden="true" />
        </Link>
      )}
      {schema.kind === 'recognition_task' && (
        <Link className="product-process-result-link" to="/security/discovery">
          {text.recognitionResults}
          <ArrowRight size={14} aria-hidden="true" />
        </Link>
      )}
    </section>
  );
}

function AssetNodes({ record, language }: { record: ProductResource; language: string }) {
  const text = processWorkspaceText(language);
  const ids = Array.isArray(record.properties.asset_ids)
    ? record.properties.asset_ids.filter((value): value is string => typeof value === 'string')
    : [];
  return (
    <section className="product-process-section" aria-label={text.assets}>
      <h4>
        <Boxes size={16} aria-hidden="true" />
        {text.assets}
      </h4>
      <p className="product-workspace-muted">{text.assetScope}</p>
      <ul className="product-asset-node-grid">
        {ids.map((id, index) => (
          <li key={`${id}-${index}`}>
            <Boxes size={19} aria-hidden="true" />
            <span>{text.assetId}</span>
            <code>{id}</code>
          </li>
        ))}
      </ul>
      {!ids.length && <p className="product-workspace-muted">{text.noAssets}</p>}
    </section>
  );
}

export default function ProcessWorkspace(props: ProductWorkspaceProps) {
  const { i18n } = useTranslation();
  const language = i18n.language;
  const text = editorWorkspaceText(language);
  const processText = processWorkspaceText(language);
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
        aria-label={localized(props.module.title, language)}
      >
        {!record ? (
          <div className="product-workspace-empty">
            <Workflow size={32} aria-hidden="true" />
            <h3>{text.noRecords}</h3>
            <p>{text.noRecordsHelp}</p>
          </div>
        ) : (
          <>
            <header className="product-definition-header">
              <div>
                <span className="product-workspace-eyebrow">
                  {localized(props.module.title, language)}
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
                <WorkspaceActions {...props} record={record} />
              </div>
            </header>
            <RecordMetadata record={record} language={language} />
            {record.description && (
              <p className="product-definition-description">{record.description}</p>
            )}
            {props.schema.kind === 'asset_map' ? (
              <AssetNodes record={record} language={language} />
            ) : (
              <ConfigurationFlow record={record} schema={props.schema} language={language} />
            )}
            <details
              className="product-process-properties"
              open={props.schema.kind === 'metadata_lineage'}
            >
              <summary>{processText.configuration}</summary>
              <div className="product-definition-fields">
                {props.schema.fields.map((field) => (
                  <section key={field.key} className="product-definition-field">
                    <h4>{localized(field.label, language)}</h4>
                    <PropertyPreview
                      schema={props.schema}
                      record={record}
                      fieldKey={field.key}
                      language={language}
                    />
                  </section>
                ))}
              </div>
            </details>
          </>
        )}
      </section>
    </div>
  );
}
