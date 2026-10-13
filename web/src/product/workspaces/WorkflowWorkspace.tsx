import { useId, useState } from 'react';
import { CalendarClock, FileCode2, GitBranch, LockKeyhole } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import {
  duplicateValues,
  findWorkflowCycle,
  missingWorkflowDependencies,
  workflowDefinition,
  type WorkflowNode,
} from '../editors/editor-models';
import type { ProductWorkspaceProps } from '../workspace-types';
import { localized, statusText } from '../ui-text';
import type { ProductResource, ResourceSchema } from '../resource-types';
import { actionAllowed, canUseResourceAction } from '../action-models';
import { useAuthStore } from '../../stores/auth';
import { Button } from '../../components/ui/button';
import { RecordMetadata } from './EditorWorkspace';
import RecordDirectory from './RecordDirectory';
import ProcessWorkspace from './ProcessWorkspace';
import { editorWorkspaceText } from './editor-workspace-text';
import { workflowLayers } from './workflow-layout';
import { WorkspaceActions, WorkspaceStatus } from './workspace-ui';
import './editor-workspaces.css';
import { useWorkflowSchedule } from '../api';

function ScheduleState({
  record,
  schema,
  canWrite,
  busy,
  onAction,
}: {
  record: ProductResource;
  schema: ResourceSchema;
  canWrite: boolean;
  busy?: boolean;
  onAction: (record: ProductResource, action: string) => void;
}) {
  const { i18n } = useTranslation();
  const schedule = useWorkflowSchedule(record.id);
  const user = useAuthStore((state) => state.user);
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  if (schedule.isPending) {
    return (
      <p className="product-workspace-muted">{text('正在读取周期计划…', 'Loading schedule…')}</p>
    );
  }
  if (schedule.error || !schedule.data) {
    return (
      <p role="alert" className="product-workflow-error">
        {text('周期计划状态读取失败', 'Unable to load schedule status')}
      </p>
    );
  }
  const { plan, state, scheduler_enabled: enabled } = schedule.data;
  const canControl = (action: string) =>
    schema.actions.includes(action) &&
    actionAllowed(record, action) &&
    canUseResourceAction(
      record.kind,
      action,
      !!user?.permissions.includes(schema.read_permission),
      canWrite,
      user?.permissions ?? [],
    );
  const canRegister =
    canControl('register_schedule') &&
    ['not_registered', 'stopped', 'registration_unknown', 'stop_unknown'].includes(state);
  const canStop =
    canControl('unschedule') && ['active', 'registration_unknown', 'stop_unknown'].includes(state);
  return (
    <section className="rounded-lg border border-[var(--color-border-default)] bg-[var(--color-bg-secondary)] p-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex items-center gap-2">
          <CalendarClock size={17} className="text-[var(--color-accent)]" aria-hidden="true" />
          <h4>{text('周期计划', 'Schedule plan')}</h4>
        </div>
        <WorkspaceStatus status={state} />
      </div>
      {!enabled && (
        <p className="mt-3 text-sm text-[var(--color-warning)]">
          {text('当前环境尚未启用 JiaScheduler。', 'JiaScheduler is disabled in this environment.')}
        </p>
      )}
      {plan ? (
        <dl className="mt-3 grid gap-3 text-sm sm:grid-cols-2 lg:grid-cols-4">
          <div>
            <dt className="text-[var(--color-text-tertiary)]">Cron</dt>
            <dd className="mt-1 font-mono">{plan.cron_expression}</dd>
          </div>
          <div>
            <dt className="text-[var(--color-text-tertiary)]">{text('时区', 'Timezone')}</dt>
            <dd className="mt-1">{plan.timezone}</dd>
          </div>
          <div>
            <dt className="text-[var(--color-text-tertiary)]">
              {text('计划版本', 'Plan version')}
            </dt>
            <dd className="mt-1">v{plan.plan_version}</dd>
          </div>
          <div>
            <dt className="text-[var(--color-text-tertiary)]">
              {text('来源版本', 'Source version')}
            </dt>
            <dd className="mt-1">v{plan.source_version}</dd>
          </div>
        </dl>
      ) : (
        <p className="mt-3 text-sm text-[var(--color-text-tertiary)]">
          {text(
            '发布工作流后，可使用“注册周期计划”把当前版本同步到调度器。',
            'Publish the workflow, then register its current version with the scheduler.',
          )}
        </p>
      )}
      {plan?.last_error && (
        <p role="alert" className="mt-3 text-sm text-[var(--color-error)]">
          {text('远端结果待核对：', 'Remote result requires reconciliation: ')}
          {plan.last_error}
        </p>
      )}
      {(canRegister || canStop) && (
        <div className="mt-4 flex flex-wrap justify-end gap-2 border-t border-[var(--color-border-default)] pt-3">
          {canStop && (
            <Button
              type="button"
              variant="outline"
              disabled={busy}
              onClick={() => onAction(record, 'unschedule')}
            >
              {statusText('unschedule', i18n.language)}
            </Button>
          )}
          {canRegister && (
            <Button
              type="button"
              disabled={busy || !enabled}
              onClick={() => onAction(record, 'register_schedule')}
            >
              {statusText('register_schedule', i18n.language)}
            </Button>
          )}
        </div>
      )}
    </section>
  );
}

interface NodeSelectionProps {
  nodes: WorkflowNode[];
  selectedId?: string;
  selectedIndex?: number;
  onSelect: (node: WorkflowNode) => void;
}

function DependencyGraph({ nodes, selectedId, onSelect }: NodeSelectionProps) {
  const { i18n } = useTranslation();
  const text = editorWorkspaceText(i18n.language);
  const markerId = `workflow-arrow-${useId().replace(/:/g, '')}`;
  const layers = workflowLayers(nodes);
  const coordinates = new Map(
    layers.flatMap((layer, depth) =>
      layer.map((node, row) => [node.id, { x: depth * 224 + 16, y: row * 100 + 44 }] as const),
    ),
  );
  const width = layers.length * 224 + 16;
  const height = Math.max(...layers.map((layer) => layer.length), 1) * 100 + 54;

  return (
    <div className="product-workflow-graph-scroll" role="group" aria-label={text.dependencyGraph}>
      <div className="product-workflow-graph-canvas" style={{ width, height }}>
        <svg
          width={width}
          height={height}
          aria-hidden="true"
          className="product-workflow-graph-edges"
        >
          <defs>
            <marker
              id={markerId}
              markerWidth="7"
              markerHeight="7"
              refX="6"
              refY="3.5"
              orient="auto"
            >
              <path d="M 0 0 L 7 3.5 L 0 7 z" fill="currentColor" />
            </marker>
          </defs>
          {nodes.flatMap((node) =>
            node.depends_on.map((dependency, index) => {
              const from = coordinates.get(dependency);
              const to = coordinates.get(node.id);
              if (!from || !to) return null;
              const x1 = from.x + 180;
              const y1 = from.y + 38;
              const x2 = to.x;
              const y2 = to.y + 38;
              const midpoint = (x1 + x2) / 2;
              return (
                <path
                  key={`${node.id}-${dependency}-${index}`}
                  d={`M ${x1} ${y1} C ${midpoint} ${y1}, ${midpoint} ${y2}, ${x2 - 4} ${y2}`}
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="1.5"
                  markerEnd={`url(#${markerId})`}
                />
              );
            }),
          )}
        </svg>
        {layers.map((layer, depth) => (
          <div key={depth}>
            <span className="product-workflow-level" style={{ left: depth * 224 + 16 }}>
              {text.level} {depth + 1}
            </span>
            {layer.map((node) => {
              const point = coordinates.get(node.id);
              return (
                <button
                  key={node.id}
                  type="button"
                  className={
                    selectedId === node.id
                      ? 'product-workflow-graph-node is-selected'
                      : 'product-workflow-graph-node'
                  }
                  style={{ left: point?.x, top: point?.y }}
                  onClick={() => onSelect(node)}
                  aria-pressed={selectedId === node.id}
                >
                  <GitBranch size={15} aria-hidden="true" />
                  <span>
                    <strong>{node.name || node.id}</strong>
                    <code>{node.id}</code>
                  </span>
                </button>
              );
            })}
          </div>
        ))}
      </div>
    </div>
  );
}

function NodeList({ nodes, selectedIndex, onSelect }: NodeSelectionProps) {
  return (
    <div className="product-workflow-invalid-nodes">
      {nodes.map((node, index) => (
        <button
          key={`${index}-${node.id}`}
          type="button"
          aria-pressed={selectedIndex === index}
          className={selectedIndex === index ? 'is-selected' : ''}
          onClick={() => onSelect(node)}
        >
          <GitBranch size={15} aria-hidden="true" />
          <span>
            <strong>{node.name || node.id || '—'}</strong>
            <code>{node.id}</code>
          </span>
        </button>
      ))}
    </div>
  );
}

function NodeDetails({
  node,
  nodes,
  onSelect,
}: {
  node: WorkflowNode;
  nodes: WorkflowNode[];
  onSelect: (node: WorkflowNode) => void;
}) {
  const { i18n } = useTranslation();
  const text = editorWorkspaceText(i18n.language);
  const relations = [
    { title: text.upstream, ids: node.depends_on },
    {
      title: text.downstream,
      ids: nodes.filter((item) => item.depends_on.includes(node.id)).map((item) => item.id),
    },
  ];
  return (
    <section className="product-workflow-node-details" aria-label={text.nodeDetails}>
      <header>
        <GitBranch size={17} aria-hidden="true" />
        <h4>{node.name || node.id}</h4>
        <code>{node.id}</code>
      </header>
      <div className="product-workflow-relations">
        {relations.map((relation) => (
          <div key={relation.title}>
            <h5>{relation.title}</h5>
            <div className="product-workflow-dependency-links">
              {!relation.ids.length && (
                <span className="product-workspace-muted">{text.noDependencies}</span>
              )}
              {relation.ids.map((id, index) => {
                const target = nodes.find((item) => item.id === id);
                return target ? (
                  <button key={`${id}-${index}`} type="button" onClick={() => onSelect(target)}>
                    {target.name || target.id}
                    <code>{target.id}</code>
                  </button>
                ) : (
                  <code className="product-workflow-missing-id" key={`${id}-${index}`}>
                    {id}
                  </code>
                );
              })}
            </div>
          </div>
        ))}
      </div>
      <h5 className="product-workflow-sql-heading">
        <FileCode2 size={15} aria-hidden="true" />
        {text.sqlDefinition}
      </h5>
      {node.sql ? (
        <pre className="product-code-preview">
          <code>{node.sql}</code>
        </pre>
      ) : (
        <p className="product-workspace-muted">{text.notConfigured}</p>
      )}
      <p className="product-workspace-muted">{text.notRun}</p>
    </section>
  );
}

function DevelopmentWorkflowWorkspace(props: ProductWorkspaceProps) {
  const { i18n } = useTranslation();
  const text = editorWorkspaceText(i18n.language);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [nodeSelection, setNodeSelection] = useState<{
    recordId: string;
    nodeId: string;
    index: number;
  } | null>(null);
  const record = props.records.find((item) => item.id === selectedId) ?? props.records[0];
  const nodes = workflowDefinition(record?.properties.definition).nodes;
  const sameRecord = record?.id === nodeSelection?.recordId;
  const selectedNode = sameRecord && nodeSelection ? nodes[nodeSelection.index] : undefined;
  const node =
    (selectedNode?.id === nodeSelection?.nodeId
      ? selectedNode
      : nodes.find((item) => sameRecord && item.id === nodeSelection?.nodeId)) ?? nodes[0];
  const selectNode = (item: WorkflowNode) => {
    if (record) {
      setNodeSelection({ recordId: record.id, nodeId: item.id, index: nodes.indexOf(item) });
    }
  };
  const cycle = findWorkflowCycle(nodes);
  const missing = missingWorkflowDependencies(nodes);
  const duplicates = duplicateValues(nodes.map((item) => item.id));
  const blankId = nodes.some((item) => !item.id.trim());
  const invalid = !!(cycle.length || missing.length || duplicates.length || blankId);

  return (
    <div className="product-editor-workspace">
      <RecordDirectory
        records={props.records}
        selectedId={record?.id}
        onSelect={(item) => setSelectedId(item.id)}
      />
      <section className="product-definition-panel" aria-label={text.workflowDefinition}>
        {!record ? (
          <div className="product-workspace-empty">
            <GitBranch size={32} aria-hidden="true" />
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
                  excludeActions={['register_schedule', 'unschedule']}
                />
              </div>
            </header>
            <RecordMetadata record={record} language={i18n.language} />
            <ScheduleState
              record={record}
              schema={props.schema}
              canWrite={props.canWrite}
              busy={props.busy}
              onAction={props.onAction}
            />
            {record.description && (
              <p className="product-definition-description">{record.description}</p>
            )}
            <div className="product-workflow-content">
              <h4>{text.dependencyGraph}</h4>
              <p className="product-workspace-muted">{text.graphHelp}</p>
              {!!cycle.length && (
                <p role="alert" className="product-workflow-error">
                  {text.cycle} {cycle.join(' → ')}
                </p>
              )}
              {!!missing.length && (
                <p role="alert" className="product-workflow-error">
                  {text.missing}: {missing.join(', ')}
                </p>
              )}
              {!!duplicates.length && (
                <p role="alert" className="product-workflow-error">
                  {text.duplicate}: {duplicates.join(', ')}
                </p>
              )}
              {blankId && (
                <p role="alert" className="product-workflow-error">
                  {text.blankId}
                </p>
              )}
              {!nodes.length ? (
                <div className="product-workflow-no-nodes">
                  <GitBranch size={25} aria-hidden="true" />
                  <p>{text.noNodes}</p>
                </div>
              ) : invalid ? (
                <>
                  <p className="product-workspace-muted">{text.invalidGraph}</p>
                  <NodeList
                    nodes={nodes}
                    selectedIndex={nodes.indexOf(node)}
                    onSelect={selectNode}
                  />
                </>
              ) : (
                <DependencyGraph nodes={nodes} selectedId={node?.id} onSelect={selectNode} />
              )}
              {node && <NodeDetails node={node} nodes={nodes} onSelect={selectNode} />}
            </div>
          </>
        )}
      </section>
    </div>
  );
}

export default function WorkflowWorkspace(props: ProductWorkspaceProps) {
  return props.schema.kind === 'development_workflow' ? (
    <DevelopmentWorkflowWorkspace {...props} />
  ) : (
    <ProcessWorkspace {...props} />
  );
}
