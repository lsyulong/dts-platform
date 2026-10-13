import client from '../../api/client';
import { apiErrorMessage } from '../../lib/apiError';
import { useAuthStore } from '../../stores/auth';
import { isRecord } from './business-models';
import { businessText } from './business-text';
import type { SelectionOption } from './types';

export type SelectionSource =
  | 'users'
  | 'roles'
  | 'organization'
  | 'system_menu'
  | 'quality_rule'
  | 'asset_entry'
  | 'atomic_metric'
  | 'derived_metric'
  | 'composite_metric';

let previousToken: string | null | undefined;
let sessionNumber = 0;

// Query keys contain a session number rather than the bearer token.
export function selectionSessionScope(token: string | null) {
  if (previousToken !== token) {
    previousToken = token;
    sessionNumber += 1;
  }
  return sessionNumber;
}

export async function fetchSelectionOptions({
  source,
  tenantId,
  userId,
  token,
  q,
  offset,
  language,
  signal,
  resourceKind,
}: {
  source: SelectionSource;
  tenantId: string;
  userId: string;
  token: string;
  q: string;
  offset: number;
  language: string;
  signal: AbortSignal;
  resourceKind?: string;
}): Promise<{ data: SelectionOption[]; total: number }> {
  const text = businessText(language);
  const systemSource = source === 'users' || source === 'roles';
  const directorySource = source === 'users' && !!resourceKind;
  try {
    const response = await client.get<unknown>(
      directorySource
        ? '/product/reference/users'
        : systemSource
          ? `/system/${source}`
          : `/product/resources/${source}`,
      {
        params: directorySource
          ? { resource_kind: resourceKind, q: q || undefined, limit: 50, offset }
          : systemSource
            ? { tenant_id: tenantId, search: q || undefined }
            : { q: q || undefined, limit: 50, offset },
        headers: { Authorization: `Bearer ${token}` },
        signal,
      },
    );
    const current = useAuthStore.getState();
    if (
      current.token !== token ||
      current.user?.tenant_id !== tenantId ||
      current.user.id !== userId
    )
      throw new Error(text.sessionChanged);
    const page = response.data;
    if (!isRecord(page) || !Array.isArray(page.data)) throw new Error(text.loadError);
    const options = page.data.map((raw): SelectionOption => {
      if (!isRecord(raw) || raw.tenant_id !== tenantId) throw new Error(text.scopeError);
      if (!systemSource && raw.kind !== source) throw new Error(text.loadError);
      if (typeof raw.id !== 'string') throw new Error(text.loadError);
      const label = source === 'users' ? raw.display_name || raw.username : raw.name;
      const detail = source === 'users' ? raw.username : systemSource ? raw.description : raw.code;
      if (typeof label !== 'string') throw new Error(text.loadError);
      return { id: raw.id, label, detail: typeof detail === 'string' ? detail : '' };
    });
    if (new Set(options.map((option) => option.id)).size !== options.length)
      throw new Error(text.loadError);
    if (systemSource && !directorySource) {
      const normalized = q.toLocaleLowerCase();
      const filtered = options.filter((option) =>
        `${option.label} ${option.detail}`.toLocaleLowerCase().includes(normalized),
      );
      return { data: filtered.slice(offset, offset + 50), total: filtered.length };
    }
    if (typeof page.total !== 'number' || !Number.isSafeInteger(page.total) || page.total < 0)
      throw new Error(text.loadError);
    return { data: options, total: page.total };
  } catch (error) {
    // Discard request configuration before storing an error in query metadata.
    throw new Error(apiErrorMessage(error, text.loadError));
  }
}
