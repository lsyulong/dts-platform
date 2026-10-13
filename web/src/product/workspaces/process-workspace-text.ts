const zh = {
  configurationFlow: '配置流程',
  flowScope: '箭头表示已配置的处理关系，不表示任务已执行或处理成功。记录状态以服务端返回为准。',
  collectionScope: '采集范围',
  collectionPlan: '采集计划',
  executionPolicy: '执行策略',
  checkpoint: '已保存消费位点',
  lineage: '已保存血缘关系',
  lineageScope: '此处展示当前记录中的上游和下游引用；来源证据与来源类型在配置详情中查看。',
  assets: '已配置资产节点',
  assetScope: '节点来自此地图保存的资产 ID；尚无关系证据时不绘制资产连线。',
  noAssets: '此地图尚未配置资产节点。',
  assetId: '资产 ID',
  qualityResults: '查看质量结果',
  recognitionResults: '查看敏感发现',
  configuration: '配置详情',
};

const en: typeof zh = {
  configurationFlow: 'Configuration flow',
  flowScope:
    'Arrows show configured processing relationships, not execution or success. Record status comes from the server.',
  collectionScope: 'Collection scope',
  collectionPlan: 'Collection plan',
  executionPolicy: 'Execution policy',
  checkpoint: 'Saved consumption checkpoint',
  lineage: 'Saved lineage relationship',
  lineageScope:
    'This view shows the upstream and downstream references saved in this record. Review configuration details for the evidence and origin.',
  assets: 'Configured asset nodes',
  assetScope:
    'Nodes use the asset IDs saved in this map. No relationships are drawn without relation evidence.',
  noAssets: 'No asset nodes are configured in this map.',
  assetId: 'Asset ID',
  qualityResults: 'View quality results',
  recognitionResults: 'View sensitive findings',
  configuration: 'Configuration details',
};

export const processWorkspaceText = (language: string) => (language.startsWith('zh') ? zh : en);
