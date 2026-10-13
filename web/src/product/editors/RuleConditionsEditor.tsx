import {
  ruleConditions,
  ruleOperators,
  type RuleCondition,
  type RuleOperator,
  type StructuredEditorProps,
} from './editor-models';
import { useEditorText } from './editor-text';
import { EditorHeader, RemoveButton } from './editor-ui';
import './editors.css';

function ScalarValueInput({
  value,
  onChange,
  label,
}: {
  value: unknown;
  onChange: (value: unknown) => void;
  label: string;
}) {
  const t = useEditorText();
  const valueType = typeof value;
  if (
    value !== null &&
    valueType !== 'string' &&
    valueType !== 'number' &&
    valueType !== 'boolean'
  ) {
    return <p className="structured-editor-help">{t.unknownValueType}</p>;
  }
  const type = valueType === 'number' || valueType === 'boolean' ? valueType : 'string';
  return (
    <div className="structured-scalar-input">
      <select
        value={type}
        aria-label={`${label} ${t.valueType}`}
        onChange={(event) =>
          onChange(
            event.target.value === 'number' ? 0 : event.target.value === 'boolean' ? false : '',
          )
        }
      >
        <option value="string">{t.stringType}</option>
        <option value="number">{t.numberType}</option>
        <option value="boolean">{t.booleanType}</option>
      </select>
      {type === 'boolean' ? (
        <select
          value={String(value)}
          aria-label={label}
          onChange={(event) => onChange(event.target.value === 'true')}
        >
          <option value="true">{t.trueValue}</option>
          <option value="false">{t.falseValue}</option>
        </select>
      ) : (
        <input
          type={type === 'number' ? 'number' : 'text'}
          step={type === 'number' ? 'any' : undefined}
          value={value === null ? '' : String(value)}
          aria-label={label}
          onChange={(event) => {
            const next = type === 'number' ? Number(event.target.value) : event.target.value;
            if (typeof next !== 'number' || Number.isFinite(next)) onChange(next);
          }}
        />
      )}
    </div>
  );
}

function ConditionValueInput({
  condition,
  onChange,
  index,
}: {
  condition: RuleCondition;
  onChange: (value: unknown) => void;
  index: number;
}) {
  const t = useEditorText();
  const label = `${t.conditionValue} ${index + 1}`;
  if (condition.operator === 'is_null' || condition.operator === 'not_null') {
    return <p className="structured-editor-help">{t.noComparisonValue}</p>;
  }
  if (condition.operator !== 'in') {
    return <ScalarValueInput value={condition.value} onChange={onChange} label={label} />;
  }
  const values = Array.isArray(condition.value) ? condition.value : [];
  return (
    <div className="structured-comparison-values">
      {values.map((value, position) => (
        <div className="structured-comparison-row" key={position}>
          <ScalarValueInput
            value={value}
            label={`${label} ${position + 1}`}
            onChange={(next) =>
              onChange(values.map((current, item) => (item === position ? next : current)))
            }
          />
          <RemoveButton
            label={`${t.removeComparisonValue} ${index + 1}.${position + 1}`}
            onRemove={() => onChange(values.filter((_, item) => item !== position))}
          />
        </div>
      ))}
      <button
        type="button"
        className="structured-editor-add"
        onClick={() => onChange([...values, ''])}
      >
        {t.addComparisonValue}
      </button>
    </div>
  );
}

function operatorValue(condition: RuleCondition, operator: RuleOperator): unknown {
  if (operator === 'is_null' || operator === 'not_null') return null;
  if (operator === 'in')
    return Array.isArray(condition.value) ? condition.value : [condition.value ?? ''];
  return Array.isArray(condition.value) ? (condition.value[0] ?? '') : (condition.value ?? '');
}

export default function RuleConditionsEditor({ value, onChange, disabled }: StructuredEditorProps) {
  const t = useEditorText();
  const conditions = ruleConditions(value);
  const update = (index: number, patch: Partial<RuleCondition>) =>
    onChange(
      conditions.map((condition, position) =>
        position === index ? { ...condition, ...patch } : condition,
      ),
    );

  return (
    <fieldset className="structured-editor" disabled={disabled}>
      <legend className="sr-only">{t.rules}</legend>
      <EditorHeader
        title={t.rules}
        addLabel={t.addCondition}
        disabled={disabled}
        onAdd={() => onChange([...conditions, { column: '', operator: 'eq', value: '' }])}
      />
      <p className="structured-editor-help">{t.allConditions}</p>
      {!conditions.length && <p className="structured-editor-empty">{t.emptyConditions}</p>}
      <div className="structured-editor-rows">
        {conditions.map((condition, index) => (
          <div className="structured-editor-row structured-condition-row" key={index}>
            <label className="structured-editor-field">
              <span>{t.column}</span>
              <input
                value={condition.column}
                aria-label={`${t.column} ${index + 1}`}
                onChange={(event) => update(index, { column: event.target.value })}
              />
            </label>
            <label className="structured-editor-field">
              <span>{t.operator}</span>
              <select
                value={condition.operator}
                aria-label={`${t.operator} ${index + 1}`}
                onChange={(event) => {
                  const operator =
                    ruleOperators.find((operator) => operator === event.target.value) ?? 'eq';
                  update(index, { operator, value: operatorValue(condition, operator) });
                }}
              >
                {ruleOperators.map((operator) => (
                  <option key={operator} value={operator}>
                    {t[operator]}
                  </option>
                ))}
              </select>
            </label>
            <div className="structured-editor-field">
              <span>{t.conditionValue}</span>
              <ConditionValueInput
                condition={condition}
                index={index}
                onChange={(value) => update(index, { value })}
              />
            </div>
            <RemoveButton
              label={`${t.removeCondition} ${index + 1}`}
              disabled={disabled}
              onRemove={() => onChange(conditions.filter((_, position) => position !== index))}
            />
          </div>
        ))}
      </div>
    </fieldset>
  );
}
