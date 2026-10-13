import { ClipboardCheck } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { formatProperty } from '../format-property';
import { localized, productText, statusText } from '../ui-text';
import type { ProductWorkspaceProps } from '../workspace-types';
import { FieldValue, WorkspaceActions, WorkspaceScope, WorkspaceStatus } from './workspace-ui';
import { workspaceText } from './workspace-text';
import { firstField } from './workspace-utils';

export default function ReviewQueueWorkspace(props: ProductWorkspaceProps) {
  const { i18n } = useTranslation();
  const language = i18n.language;
  const text = productText(language);
  const words = workspaceText(language);
  const applicantField = firstField(props.schema, ['applicant_id', 'requester_id', 'assignee_id']);
  const resourceField = firstField(props.schema, [
    'object_id',
    'request_id',
    'api_id',
    'related_id',
    'task_id',
    'policy_id',
  ]);
  const reasonField = firstField(props.schema, [
    'purpose',
    'decision_reason',
    'evidence_summary',
    'evidence',
  ]);
  const states = [...new Set(props.schema.statuses)];
  const ownRequests = ['data_access_request', 'service_subscription'].includes(props.schema.kind);

  return (
    <div>
      <div className="space-y-3 border-b border-[var(--color-border-default)] px-4 pt-4 sm:px-5">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <h2 className="flex items-center gap-2 text-sm font-semibold">
            <ClipboardCheck size={16} className="text-[var(--color-accent)]" />
            {localized(props.module.title, language)}
          </h2>
          <span className="text-xs text-[var(--color-text-tertiary)]">
            {words.currentPage}: {props.records.length}
          </span>
        </div>
        <WorkspaceScope language={language} />
        <div className="flex gap-1 overflow-x-auto" role="tablist" aria-label={words.reviewState}>
          {['', ...states].map((state) => {
            const active = (props.status ?? '') === state;
            return (
              <button
                key={state}
                type="button"
                role="tab"
                aria-selected={active}
                disabled={props.busy}
                onClick={() => props.onStatusChange(state)}
                className={
                  'whitespace-nowrap border-b-2 px-4 py-3 text-sm font-medium transition-colors disabled:opacity-40 ' +
                  (active
                    ? 'border-[var(--color-accent)] text-[var(--color-accent)]'
                    : 'border-transparent text-[var(--color-text-secondary)] hover:text-[var(--color-accent)]')
                }
              >
                {state ? statusText(state, language) : text.allStatus}
              </button>
            );
          })}
        </div>
      </div>
      <div className="overflow-x-auto">
        <table className="data-table">
          <thead>
            <tr>
              <th>{text.name}</th>
              <th>
                {applicantField ? localized(applicantField.label, language) : words.applicant}
              </th>
              <th>
                {resourceField ? localized(resourceField.label, language) : words.requestedResource}
              </th>
              <th>
                {reasonField ? localized(reasonField.label, language) : words.applicationReason}
              </th>
              <th>{words.reviewState}</th>
              <th>{text.operations}</th>
            </tr>
          </thead>
          <tbody>
            {props.records.map((record) => {
              const applicant =
                record.properties.applicant_id ??
                record.properties.requester_id ??
                (ownRequests ? record.created_by : undefined);
              return (
                <tr key={record.id}>
                  <td className="min-w-44">
                    <button
                      type="button"
                      className="font-medium text-[var(--color-accent)] hover:underline"
                      onClick={() => props.onView(record)}
                    >
                      {record.name}
                    </button>
                    <p className="mt-1 text-xs text-[var(--color-text-tertiary)]">{record.code}</p>
                    <p className="mt-1 whitespace-nowrap text-xs text-[var(--color-text-tertiary)]">
                      {words.created}: {new Date(record.created_at).toLocaleString(language)}
                    </p>
                  </td>
                  <td className="max-w-48 break-all">
                    {applicantField ? (
                      <FieldValue field={applicantField} record={record} language={language} />
                    ) : (
                      formatProperty(applicant, language)
                    )}
                  </td>
                  <td className="max-w-48 break-all">
                    {resourceField ? (
                      <FieldValue field={resourceField} record={record} language={language} />
                    ) : (
                      '—'
                    )}
                  </td>
                  <td className="min-w-52 max-w-80">
                    <div className="line-clamp-3 whitespace-pre-wrap break-words text-sm text-[var(--color-text-secondary)]">
                      {reasonField ? (
                        <FieldValue field={reasonField} record={record} language={language} />
                      ) : (
                        record.description || '—'
                      )}
                    </div>
                  </td>
                  <td>
                    <WorkspaceStatus status={record.status} />
                  </td>
                  <td>
                    <WorkspaceActions {...props} record={record} />
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
}
