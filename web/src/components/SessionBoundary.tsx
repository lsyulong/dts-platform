import { useEffect } from 'react';
import { Navigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { useAuthStore } from '../stores/auth';
import { ErrorBanner, LoadingSpinner } from './ui';

export default function SessionBoundary({ children }: { children: React.ReactNode }) {
  const { t } = useTranslation();
  const { token, initialized, restoring, restoreError, fetchMe } = useAuthStore();
  useEffect(() => {
    if (token && !initialized) void fetchMe();
  }, [token, initialized, fetchMe]);
  if (!token) return <Navigate to="/login" replace />;
  if (restoreError && !restoring)
    return (
      <div className="p-6">
        <ErrorBanner
          message={`${t('auth.restoreFailed')}: ${restoreError}`}
          onRetry={() => void fetchMe()}
        />
      </div>
    );
  if (!initialized || restoring) return <LoadingSpinner />;
  return <>{children}</>;
}
