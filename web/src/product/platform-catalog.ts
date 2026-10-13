import { productText as t, type ProductGroup, type ProductModule } from './catalog-types';
import { groupDescriptions } from './catalog-copy';
import { integrationDevelopmentModules } from './integration-development-catalog';
import { warehouseMetricModules } from './warehouse-metric-catalog';
import { governanceModules } from './governance-catalog';
import { assetsServiceModules } from './assets-service-catalog';

const group = (id: string, zh: string, en: string, description: string): ProductGroup => ({
  id,
  title: t(zh, en),
  description: t(description, groupDescriptions[id] ?? en),
});

export const platformGroups: ProductGroup[] = [
  group('workbench', '工作台', 'Workbench', '工作概览、待办与个人信息'),
  group('integration', '数据集成', 'Data integration', '连接数据源、定义同步任务并跟踪执行'),
  group(
    'warehouse',
    '数仓设计',
    'Warehouse design',
    '业务分类、数仓分层、数据域、过程、集市与主题',
  ),
  group('standards', '数据标准', 'Data standards', '统一命名、字段、枚举与计量单位'),
  group('models', '模型设计', 'Data models', '设计、导入、校验和发布数据模型'),
  group('metrics', '数据指标', 'Metrics', '原子、派生、复合指标与计算口径'),
  group('development', '数据开发', 'Data development', 'SQL 开发、工作流、任务调度和运行监控'),
  group('metadata', '元数据', 'Metadata', '采集、检索和分析数据结构与血缘'),
  group('master-data', '主数据', 'Master data', '主数据模型、编码、维护与分发'),
  group('data-security', '数据权限', 'Data access', '数据访问申请、审批和审计'),
  group('sensitive-data', '敏感数据', 'Sensitive data', '数据密级、识别规则、识别任务与发现结果'),
  group('privacy', '数据隐私', 'Data privacy', '字段脱敏规则与用户组授权'),
  group('watermark', '数据水印', 'Data watermark', '水印策略与溯源证据'),
  group('quality', '数据质量', 'Data quality', '质量规则、校验任务、结果与报告'),
  group('assets', '数据资产', 'Data assets', '资产地图、目录与使用分析'),
  group('services', '数据服务', 'Data services', 'API 定义、授权应用与调用记录'),
  group('bi', 'BI 分析', 'BI analysis', '人员分析与可复用分析报告'),
  group('support', '辅助管理', 'Supporting tools', '标签与告警配置'),
];

export const platformModules: ProductModule[] = [
  ...integrationDevelopmentModules,
  ...warehouseMetricModules,
  ...governanceModules,
  ...assetsServiceModules,
];
