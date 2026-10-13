import { useTranslation } from 'react-i18next';
import { Database, Layers3, Network, Plug, ShieldCheck, Workflow } from 'lucide-react';

const productAreas = ['integration', 'development', 'governance', 'assets', 'services', 'identity'];
const workflowSteps = [
  { key: 'sources', icon: Plug },
  { key: 'workspace', icon: Database },
  { key: 'consumption', icon: Layers3 },
] as const;

function WorkflowIllustration() {
  const { t } = useTranslation();

  return (
    <div className="login-workflow" role="img" aria-label={t('auth.workflowLabel')}>
      <div className="login-workflow-caption" aria-hidden="true">
        <Workflow size={15} />
        <span>{t('auth.workspaceLabel')}</span>
      </div>
      <div className="login-workflow-steps" aria-hidden="true">
        {workflowSteps.map(({ key, icon: Icon }, index) => (
          <div key={key} className={`login-workflow-node login-workflow-node-${index}`}>
            <div className="login-workflow-icon">
              <Icon size={25} strokeWidth={1.6} />
            </div>
            <strong>{t(`auth.workflow.${key}.title`)}</strong>
            <span>{t(`auth.workflow.${key}.description`)}</span>
          </div>
        ))}
      </div>
      <div className="login-workflow-foundation" aria-hidden="true">
        <ShieldCheck size={15} />
        <span>{t('auth.workflowFoundation')}</span>
      </div>
    </div>
  );
}

export default function LoginBrandPanel() {
  const { t } = useTranslation();

  return (
    <section className="login-brand-panel" aria-label={t('auth.productAreasLabel')}>
      <div className="login-brand">
        <span className="login-brand-mark" aria-hidden="true">
          <Network size={27} strokeWidth={1.8} />
        </span>
        <div>
          <h1>{t('common.appName')}</h1>
          <p>{t('common.appDesc')}</p>
        </div>
      </div>

      <div className="login-brand-content">
        <span className="login-brand-eyebrow">DATA · WORKSPACE</span>
        <h2>{t('auth.brandHeadline')}</h2>
        <p className="login-brand-description">{t('auth.brandDescription')}</p>
        <WorkflowIllustration />
        <div className="login-product-areas" aria-label={t('auth.productAreasLabel')}>
          {productAreas.map((area) => (
            <span key={area}>{t(`auth.productAreas.${area}`)}</span>
          ))}
        </div>
      </div>

      <p className="login-brand-footer">{t('auth.brandFooter')}</p>
    </section>
  );
}
