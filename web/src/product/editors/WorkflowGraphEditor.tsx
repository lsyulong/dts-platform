import { ArrowRight, GitBranch } from 'lucide-react';
import {
  duplicateValues,
  findWorkflowCycle,
  missingWorkflowDependencies,
  nextWorkflowNodeId,
  removeWorkflowNode,
  workflowDefinition,
  type StructuredEditorProps,
  type WorkflowNode,
} from './editor-models';
import { useEditorText } from './editor-text';
import { EditorHeader, EditorWarning, RemoveButton } from './editor-ui';
import './editors.css';
import SqlParametersEditor from '../business-editors/SqlParametersEditor';

function WorkflowOverview({ nodes }: { nodes: WorkflowNode[] }) {
  const t = useEditorText();
  if (!nodes.length) return null;
  return (
    <ol className="structured-workflow-overview" aria-label={t.workflow}>
      {nodes.map((node, index) => (
        <li key={`${index}-${node.id}`}>
          <GitBranch size={16} aria-hidden="true" />
          <div>
            <strong>{node.name || node.id || `${t.item} ${index + 1}`}</strong>
            {!!node.depends_on.length && (
              <span>
                {node.depends_on.join(', ')}
                <ArrowRight size={12} aria-hidden="true" />
                {node.id}
              </span>
            )}
          </div>
        </li>
      ))}
    </ol>
  );
}

function NodeDependencies({
  node,
  nodes,
  index,
  onChange,
}: {
  node: WorkflowNode;
  nodes: WorkflowNode[];
  index: number;
  onChange: (dependencies: string[]) => void;
}) {
  const t = useEditorText();
  const available = nodes.filter(
    (item, position) => position !== index || node.depends_on.includes(item.id),
  );
  const missing = node.depends_on.filter((id) => !nodes.some((item) => item.id === id));
  const options = [
    ...available.map((item) => ({
      id: item.id,
      label: item.name ? `${item.name} (${item.id})` : item.id,
    })),
    ...missing.map((id) => ({ id, label: `${id} — ${t.missingDependency}` })),
  ].filter(
    (item, position, values) =>
      item.id && values.findIndex((value) => value.id === item.id) === position,
  );
  return (
    <fieldset className="structured-workflow-dependencies">
      <legend>
        {t.dependencies} — {node.name || node.id}
      </legend>
      {!options.length && <p className="structured-editor-help">{t.noDependencies}</p>}
      {options.map((option) => (
        <label className="structured-editor-check" key={option.id}>
          <input
            type="checkbox"
            checked={node.depends_on.includes(option.id)}
            onChange={(event) =>
              onChange(
                event.target.checked
                  ? [...node.depends_on, option.id]
                  : node.depends_on.filter((id) => id !== option.id),
              )
            }
          />
          <span>{option.label}</span>
        </label>
      ))}
    </fieldset>
  );
}

export default function WorkflowGraphEditor({ value, onChange, disabled }: StructuredEditorProps) {
  const t = useEditorText();
  const definition = workflowDefinition(value);
  const nodes = definition.nodes;
  const cycle = findWorkflowCycle(nodes);
  const duplicates = duplicateValues(nodes.map((node) => node.id));
  const missing = missingWorkflowDependencies(nodes);
  const update = (index: number, patch: Partial<WorkflowNode>) =>
    onChange({
      ...definition,
      nodes: nodes.map((node, position) => (position === index ? { ...node, ...patch } : node)),
    });

  return (
    <fieldset className="structured-editor" disabled={disabled}>
      <legend className="sr-only">{t.workflow}</legend>
      <EditorHeader
        title={t.workflow}
        addLabel={t.addNode}
        disabled={disabled}
        onAdd={() =>
          onChange({
            ...definition,
            nodes: [...nodes, { id: nextWorkflowNodeId(nodes), name: '', sql: '', depends_on: [] }],
          })
        }
      />
      <p className="structured-editor-help">{t.workflowHelp}</p>
      <SqlParametersEditor
        kind="development_workflow"
        fieldKey="parameters"
        value={definition.parameters}
        onChange={(parameters) => onChange({ ...definition, parameters })}
        disabled={disabled}
      />
      {!!cycle.length && (
        <EditorWarning>
          {t.cycle}: {cycle.join(' → ')}
        </EditorWarning>
      )}
      {!!duplicates.length && (
        <EditorWarning>
          {t.duplicateNode}: {duplicates.join(', ')}
        </EditorWarning>
      )}
      {!!missing.length && (
        <EditorWarning>
          {t.missingDependency}: {missing.join(', ')}
        </EditorWarning>
      )}
      {nodes.some((node) => !node.id.trim()) && <EditorWarning>{t.blankNodeId}</EditorWarning>}
      {!nodes.length && <p className="structured-editor-empty">{t.noNodes}</p>}
      <WorkflowOverview nodes={nodes} />
      <div className="structured-editor-rows">
        {nodes.map((node, index) => (
          <div className="structured-workflow-node" key={`${index}-${node.id}`}>
            <div className="structured-workflow-node-header">
              <span>
                {t.nodeId}: <code>{node.id}</code>
              </span>
              <RemoveButton
                label={`${t.removeNode} ${node.name || node.id || index + 1}`}
                disabled={disabled}
                onRemove={() => onChange(removeWorkflowNode(definition, node.id, index))}
              />
            </div>
            <label className="structured-editor-field">
              <span>{t.nodeName}</span>
              <input
                value={node.name}
                aria-label={`${t.nodeName} ${index + 1}`}
                onChange={(event) => update(index, { name: event.target.value })}
              />
            </label>
            <label className="structured-editor-field">
              <span>{t.sql}</span>
              <textarea
                value={node.sql}
                rows={4}
                spellCheck={false}
                aria-label={`${t.sql} ${index + 1}`}
                onChange={(event) => update(index, { sql: event.target.value })}
              />
            </label>
            <NodeDependencies
              node={node}
              nodes={nodes}
              index={index}
              onChange={(depends_on) => update(index, { depends_on })}
            />
          </div>
        ))}
      </div>
    </fieldset>
  );
}
