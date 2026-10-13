import ModelColumnsEditor from '../editors/ModelColumnsEditor';
import type { DesignEditorProps } from './design-models';
import { useDesignText } from './design-text';

export default function ApiResponseSchemaEditor(props: DesignEditorProps) {
  const text = useDesignText();
  return (
    <section aria-label={text('API 返回结构', 'API response schema')}>
      <p className="mb-3 text-xs leading-5 text-[var(--color-text-tertiary)]">
        {text(
          '声明 API 返回字段的名称、数据类型、空值与说明。发布和调用必须核对实际查询结果；此声明不会生成查询结果。',
          'Declare API result field names, data types, nullability and descriptions. Publication and calls must verify the actual query result; this declaration does not generate results.',
        )}
      </p>
      <ModelColumnsEditor value={props.value} onChange={props.onChange} disabled={props.disabled} />
    </section>
  );
}
