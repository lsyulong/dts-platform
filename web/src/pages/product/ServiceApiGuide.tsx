import { useState } from 'react';
import { Link } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import client from '../../api/client';
import { Button } from '../../components/ui/button';
import { ErrorBanner } from '../../components/ui';
import ReferencePicker from '../../product/ReferencePicker';
import ActionParameters from '../../product/ActionParameters';
import type { ProductResource } from '../../product/resource-types';
import { useAuthStore } from '../../stores/auth';
import { apiErrorMessage } from '../../lib/apiError';
import { useServicePage } from './service-access-api';

export default function ServiceApiGuide() {
  const identity = useAuthStore((state) =>
    JSON.stringify([state.user?.tenant_id, state.user?.id, state.token, state.user?.permissions]),
  );
  return <Guide key={identity} />;
}
function Guide() {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const canRead = useAuthStore((state) => !!state.user?.permissions.includes('services:read'));
  const [id, setId] = useState(''),
    [parameters, setParameters] = useState<Record<string, unknown>>({}),
    [copied, setCopied] = useState(false),
    [copyError, setCopyError] = useState('');
  const source = useServicePage<ProductResource>(
    `/product/resources/data_api/${encodeURIComponent(id)}`,
    {},
    canRead && !!id,
  );
  const api = !source.error && source.data?.id === id ? source.data : null;
  const endpoint = `${(client.defaults.baseURL ?? '/api/v1').replace(/\/$/, '')}/data-services/${encodeURIComponent(id)}/invoke`;
  const body = JSON.stringify({ parameters }, null, 2);
  if (!canRead)
    return (
      <ErrorBanner
        message={text('当前账号没有数据服务查看权限', 'Your account cannot read data services')}
      />
    );
  return (
    <div className="space-y-5">
      <section className="data-panel p-5 space-y-4">
        <h2 className="font-semibold">{text('应用接入', 'Application integration')}</h2>
        <p className="text-sm text-[var(--color-text-secondary)]">
          {text(
            '创建应用并签发调用凭据，再申请对应 API 版本的订阅。审批通过后，业务系统即可按以下协议调用。',
            'Create an application credential, then request a subscription to the API version. Once approved, your system can invoke this endpoint.',
          )}
        </p>
        <div className="flex gap-4 text-sm text-[var(--color-accent)]">
          <Link to="/services/applications">
            {text('应用与凭据', 'Applications and credentials')}
          </Link>
          <Link to="/services/catalog">{text('订阅与审批', 'Subscriptions and approvals')}</Link>
          <Link to="/services/calls">{text('实际调用记录', 'Call audit')}</Link>
        </div>
        <ReferencePicker
          kind="data_api"
          label={text('数据 API', 'Data API')}
          value={id}
          onChange={(next) => {
            setId(next);
            setParameters({});
            setCopied(false);
          }}
        />
        {copyError && <ErrorBanner message={copyError} />}
        {source.error && (
          <ErrorBanner
            message={apiErrorMessage(
              source.error,
              text('API 定义读取失败', 'Could not read API definition'),
            )}
          />
        )}
        {api && api.status !== 'published' && (
          <ErrorBanner
            message={text(
              '该 API 尚未发布或已撤回，当前不可调用。',
              'This API is unpublished or withdrawn and cannot be invoked.',
            )}
          />
        )}
        {api?.status === 'published' && (
          <>
            <p className="text-sm">
              {api.name} · v{api.version}
            </p>
            <ActionParameters
              record={api}
              value={parameters}
              onChange={setParameters}
              disabled={false}
              text={text}
            />
            <div className="rounded-md bg-[var(--color-bg-page)] p-4 space-y-3 text-sm">
              <p className="break-all font-mono">POST {endpoint}</p>
              <p className="font-mono">Content-Type: application/json</p>
              <p className="font-mono">
                X-Application-Key: &lt;{text('首次签发的应用凭据', 'issued application credential')}
                &gt;
              </p>
              <pre className="overflow-auto whitespace-pre-wrap text-xs">{body}</pre>
              <Button
                variant="outline"
                onClick={async () => {
                  setCopyError('');
                  try {
                    await navigator.clipboard.writeText(body);
                    setCopied(true);
                  } catch {
                    setCopyError(
                      text(
                        '浏览器未允许复制，请手动复制请求体。',
                        'Clipboard access is unavailable. Copy the request body manually.',
                      ),
                    );
                  }
                }}
              >
                {copied
                  ? text('已复制请求体', 'Request body copied')
                  : text('复制请求体', 'Copy request body')}
              </Button>
            </div>
            <p className="text-sm text-[var(--color-text-secondary)]">
              {text(
                '调用使用应用凭据对应的负责人权限和当前脱敏规则。已批准订阅绑定此版本；API 重新发布后需要重新订阅。',
                'Calls use the application owner’s current data access and masking policies. Subscriptions bind this version and require renewal after republication.',
              )}
            </p>
          </>
        )}
      </section>
      <section className="data-panel p-5 space-y-3 text-sm">
        <h2 className="font-semibold">{text('响应与失败处理', 'Responses and failures')}</h2>
        <p>
          {text(
            '成功响应包含实际 rows、row_count、effective_response_schema、API 版本及 call.id。脱敏改变数值类型时，以有效响应字段类型为准。',
            'Success includes actual rows, row_count, effective_response_schema, API version and call.id. Use the effective schema when masking changes numeric fields to text.',
          )}
        </p>
        <p>
          {text(
            '403 表示凭据、订阅、来源或数据权限不可用；409 表示版本或发布状态变化；429 携带 Retry-After 等待秒数。返回行数或完整响应超过上限会失败，不返回截断的成功数据。',
            '403 means credential, subscription, origin or data access is unavailable. 409 indicates a changed version or publication. 429 provides Retry-After seconds. Oversized results fail without a truncated success response.',
          )}
        </p>
        <p>
          {text(
            '每次获得调用许可的请求都会预留分钟额度。网络超时不表示远端查询已取消，应核对实际调用记录；重新发送并获准会产生新的调用并再次计入额度。',
            'Each permitted request reserves quota. A network timeout does not imply engine cancellation; inspect call audit. An accepted retry creates a new invocation and consumes quota again.',
          )}
        </p>
      </section>
    </div>
  );
}
