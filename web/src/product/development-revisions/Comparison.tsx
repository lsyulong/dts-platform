import { useTranslation } from 'react-i18next';
import { ErrorBanner } from '../../components/ui';
import { useRevisionDiff } from './api';
import type { RevisionKind } from './types';
export default function RevisionComparison({
  kind,
  resourceId,
  from,
  to,
}: {
  kind: RevisionKind;
  resourceId: string;
  from: string;
  to: string;
}) {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const query = useRevisionDiff(kind, resourceId, from, to);
  if (query.error)
    return (
      <ErrorBanner
        message={text('修订对比读取失败，请重试', 'Failed to load revision comparison')}
      />
    );
  if (!query.data)
    return <p>{text('正在读取真实修订差异…', 'Loading actual revision differences…')}</p>;
  const data = query.data;
  return (
    <section className="data-panel p-4 space-y-3">
      <p>
        {text('可编辑定义对比', 'Editable definition comparison')} · v{data.from_version} → v
        {data.to_version}
        {data.to_revision_id === null ? text('（当前定义）', ' (current definition)') : ''}
      </p>
      <p className="text-xs break-all">
        SHA256: {data.from_hash} → {data.to_hash}
      </p>
      {data.truncated && (
        <ErrorBanner
          message={text(
            '差异达到 256 项上限，当前对比不完整',
            'The 256-change limit was reached; comparison is incomplete',
          )}
        />
      )}
      {data.changes.length ? (
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead>
              <tr>
                <th>{text('字段路径', 'Field path')}</th>
                <th>{text('原定义', 'Before')}</th>
                <th>{text('目标定义', 'After')}</th>
              </tr>
            </thead>
            <tbody>
              {data.changes.map((change) => (
                <tr key={change.path}>
                  <td className="align-top p-2 font-mono">{change.path}</td>
                  <td className="p-2 align-top">
                    <pre className="whitespace-pre-wrap break-all max-w-md">
                      {JSON.stringify(change.before, null, 2)}
                    </pre>
                  </td>
                  <td className="p-2 align-top">
                    <pre className="whitespace-pre-wrap break-all max-w-md">
                      {JSON.stringify(change.after, null, 2)}
                    </pre>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : (
        <p>
          {text('所选可编辑定义没有差异', 'The selected editable definitions have no differences')}
        </p>
      )}
    </section>
  );
}
