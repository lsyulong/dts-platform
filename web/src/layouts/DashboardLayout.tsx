import { useState } from 'react';
import { Outlet, useLocation } from 'react-router-dom';
import Sidebar from '../components/Sidebar';
import Header from '../components/Header';
import { AiFloatingButton } from '../components/ai/AiFloatingButton';
import { AiChatPanel } from '../components/ai/AiChatPanel';
import { useAiChat } from '../hooks/useAiChat';
import { useAuthStore } from '../stores/auth';
import { NavigationProvider, useNavigation } from '../product/navigation';
import { ErrorBanner } from '../components/ui';
import { Button } from '../components/ui/button';
import { useTranslation } from 'react-i18next';
import { apiErrorMessage } from '../lib/apiError';

export default function DashboardLayout() {
  return (
    <NavigationProvider>
      <WorkspaceLayout />
    </NavigationProvider>
  );
}

function WorkspaceLayout() {
  const location = useLocation();
  const user = useAuthStore((state) => state.user);
  const [aiOpen, setAiOpen] = useState(false);
  const { messages, sendMessage, isLoading, isUnavailable } = useAiChat();
  const navigation = useNavigation();
  const { i18n } = useTranslation();
  const zh = i18n.language.startsWith('zh');
  const canonicalPath =
    location.pathname === '/system'
      ? '/system/users'
      : location.pathname === '/monitoring'
        ? '/system/monitoring'
        : location.pathname === '/ops'
          ? '/integration/operations'
          : location.pathname === '/analysis'
            ? '/bi/personnel'
            : location.pathname;

  return (
    <div className="flex h-dvh bg-[var(--color-bg-secondary)] text-[var(--color-text-primary)]">
      <Sidebar navigation={navigation} />
      <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
        <Header />
        <main
          className="workspace-content flex-1 overflow-y-auto p-3 sm:p-5 lg:p-6"
          key={`${user?.tenant_id}:${user?.id}:${location.pathname}`}
        >
          <div className="page-transition">
            {navigation.phase === 'loading' ? (
              <p role="status" className="p-6 text-sm">
                {zh ? '正在读取当前租户菜单…' : 'Loading tenant navigation…'}
              </p>
            ) : navigation.phase === 'error' ? (
              <div className="space-y-4">
                <ErrorBanner
                  message={apiErrorMessage(
                    navigation.error,
                    zh ? '当前租户菜单读取失败' : 'Could not read tenant navigation',
                  )}
                />
                <Button variant="outline" onClick={() => void navigation.refresh()}>
                  {zh ? '重新读取' : 'Retry'}
                </Button>
              </div>
            ) : navigation.allowsPath(canonicalPath) ? (
              <Outlet />
            ) : (
              <div
                role="alert"
                className="rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-6 text-sm"
              >
                {zh
                  ? '当前菜单策略或账号权限未开放此页面。请从左侧可用菜单进入，管理员可在菜单模板中调整。'
                  : 'This page is unavailable under the current navigation policy or account permissions. Choose an available menu; administrators can update menu templates.'}
              </div>
            )}
          </div>
        </main>
      </div>
      <AiFloatingButton onClick={() => setAiOpen(true)} isOpen={aiOpen} />
      {aiOpen && (
        <AiChatPanel
          messages={messages}
          onSend={sendMessage}
          onClose={() => setAiOpen(false)}
          isLoading={isLoading}
          isUnavailable={isUnavailable}
        />
      )}
    </div>
  );
}
