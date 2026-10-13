import { useState } from 'react';
import { FolderOpen, Search } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import type { ProductResource } from '../resource-types';
import { editorWorkspaceText } from './editor-workspace-text';
import { WorkspaceScope, WorkspaceStatus } from './workspace-ui';
import './editor-workspaces.css';

export default function RecordDirectory({
  records,
  selectedId,
  onSelect,
}: {
  records: ProductResource[];
  selectedId?: string;
  onSelect: (record: ProductResource) => void;
}) {
  const { i18n } = useTranslation();
  const text = editorWorkspaceText(i18n.language);
  const [search, setSearch] = useState('');
  const query = search.trim().toLocaleLowerCase();
  const shown = records.filter(
    (record) => !query || `${record.code} ${record.name}`.toLocaleLowerCase().includes(query),
  );
  return (
    <aside className="product-record-directory" aria-label={text.directory}>
      <div className="product-directory-heading">
        <FolderOpen size={16} aria-hidden="true" />
        <h3>{text.directory}</h3>
        <span>{records.length}</span>
      </div>
      <label className="product-directory-search">
        <Search size={14} aria-hidden="true" />
        <input
          value={search}
          onChange={(event) => setSearch(event.target.value)}
          aria-label={text.directorySearch}
          placeholder={text.directorySearch}
        />
      </label>
      <ul className="product-directory-records">
        {shown.map((record) => (
          <li key={record.id}>
            <button
              type="button"
              aria-pressed={record.id === selectedId}
              className={record.id === selectedId ? 'is-selected' : ''}
              onClick={() => onSelect(record)}
            >
              <strong>{record.name}</strong>
              <code>{record.code}</code>
              <WorkspaceStatus status={record.status} />
            </button>
          </li>
        ))}
      </ul>
      {!shown.length && (
        <p className="product-workspace-muted">{records.length ? text.noMatch : text.noRecords}</p>
      )}
      <div className="product-directory-scope">
        <WorkspaceScope language={i18n.language} />
      </div>
    </aside>
  );
}
