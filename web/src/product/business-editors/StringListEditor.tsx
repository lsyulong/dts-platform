import { useTranslation } from 'react-i18next';
import type { BusinessPropertyEditorProps } from './types';
import { duplicateIndexes, stringList } from './business-models';
import { businessText } from './business-text';
import {
  BusinessEditorFrame,
  BusinessRemove,
  BusinessWarning,
  inputClass,
  InvalidBusinessValue,
} from './business-ui';

export default function StringListEditor({
  value,
  onChange,
  disabled,
}: BusinessPropertyEditorProps) {
  const { i18n } = useTranslation();
  const text = businessText(i18n.language);
  const items = stringList(value);
  if (!items)
    return (
      <InvalidBusinessValue
        text={text.incompatible}
        resetLabel={text.reset}
        onReset={() => onChange([])}
        disabled={disabled}
      />
    );
  const duplicates = duplicateIndexes(items);
  return (
    <BusinessEditorFrame
      title={text.stringItems}
      addLabel={text.add}
      onAdd={() => onChange([...items, ''])}
      disabled={disabled}
    >
      {items.map((item, index) => (
        <div key={index} className="flex items-start gap-2">
          <input
            type="text"
            aria-label={`${text.item} ${index + 1}`}
            className={inputClass}
            value={item}
            required
            disabled={disabled}
            aria-invalid={duplicates.has(index)}
            ref={(node) =>
              node?.setCustomValidity(
                duplicates.has(index) ? text.duplicate : item.trim() ? '' : text.missing,
              )
            }
            onChange={(event) =>
              onChange(
                items.map((current, currentIndex) =>
                  currentIndex === index ? event.target.value : current,
                ),
              )
            }
          />
          <BusinessRemove
            label={`${text.remove} ${text.item} ${index + 1}`}
            disabled={disabled}
            onRemove={() => onChange(items.filter((_, currentIndex) => currentIndex !== index))}
          />
        </div>
      ))}
      {duplicates.size > 0 && <BusinessWarning>{text.duplicate}</BusinessWarning>}
      {!items.length && (
        <p className="py-3 text-center text-xs text-[var(--color-text-tertiary)]">{text.empty}</p>
      )}
    </BusinessEditorFrame>
  );
}
