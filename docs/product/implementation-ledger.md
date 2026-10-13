# 产品实施台账

盘点日期：2026-10-13，Asia/Shanghai。本文依据当前工作区源码盘点，不是发布验收报告。用户要求先完成两套产品对应的前后端业务功能，再集中做整体 POC；**整体范围尚未完成，整体 POC 尚未开展**。

## 计数与判定口径

当前目录有 19 个分组、83 个业务叶模块，其中数据产品 63 个、公共权限产品 20 个。事实源为 [后端目录](../../crates/dts-server/resources/product/catalog.json)、[前端目录](../../web/src/product/catalog.ts) 和 [完整功能合同](full-scope-contract.md)。两份购买原型包含的 206 个画布还包含说明、表单、弹窗及页面变体，不能与 83 个模块或生产路由直接等同；目前没有完成全部画布的完整交互还原。

83 个模块已有路由与统一绿色控制台入口，公共权限产品使用同一风格。入口分别落到已有业务页面、专用页面或通用工作区；“树/编辑器/审批/监控/资产目录/工作流”工作区已经分型，但这不表示该模块的领域流程均已实现。[路由分派](../../web/src/product/ProductRoute.tsx)、[路由注册](../../web/src/router.tsx)、[工作区分派](../../web/src/product/ProductWorkspace.tsx) 是 UI 依据。

本表分别记录 UI、业务存储与规则、执行能力及合同缺口。使用以下口径：

- **定义管理**：资源保存、字段类型校验、引用、租户隔离、版本及审计；这些能力不能单独算完整业务功能。
- **领域流程源码已接**：有专门服务/仓储、实际状态转换与来源约束。仍需对应测试和产品流程验收。
- **执行源码已接**：有真实执行入口、实例和结果处理；依赖引擎的能力仍需实际环境验证。“已接”不表示本机运行成功或生产验收完成。
- **未接**：目标动作当前未进入实际执行/消费链。前端目录中的 `actions` 是目标，实际可用动作由 [服务端 schema](../../crates/dts-server/src/services/product/catalog.rs)、[动作分派](../../crates/dts-server/src/api/product_actions.rs) 和具体服务决定；未实现动作不会因为目录里有标签而完成。

SQLite 是本地控制面存储，PostgreSQL 是生产控制面驱动，DuckDB/SwanLake 是数据平面。持久化只读任务的 lease/fence 提供 worker 协调基础，不能据此宣称已实现分布式 SQL、分布式湖仓、CDC 或写任务的精确一次提交。通用页面“导出当前页”只导出当前列表的基础信息，不等同于合同中的数据结果、完整审计或质量报告导出。

## 工作台与数据集成（5）

依据：[工作台](../../web/src/pages/dashboard.tsx)、[数据源 API](../../crates/dts-server/src/api/datasources.rs)、[同步任务服务](../../crates/dts-server/src/services/task_svc.rs)、[同步执行调度](../../crates/dts-server/src/services/scheduler/mod.rs)、[任务日志](profile-and-run-log-contract.md)、[JiaScheduler 接线](../../crates/dts-server/src/api/scheduler.rs)。

| 模块 / 路径 | UI 当前实现 | 真实业务当前实现 | 运行能力 | 合同缺口 |
|---|---|---|---|---|
| 工作台 `/` | 已有概览、近期运行与导航 | 当前租户同步任务、数据源、运行仓储汇总；真实空态 | 读取控制面记录 | 未汇总全部新增域与统一待办；部分指标是任务定义状态口径，不能当全部历史运行或企业活跃统计 |
| 数据源管理 `/datasources` | 已有连接表单、列表、真实连接测试与最多500张基础表目录选择 | 真实 datasource 仓储，凭据保护、租户范围及 MySQL/PostgreSQL/DuckDB 目录读取 | MySQL/PostgreSQL/SQLite 探测与 SwanLake Flight SQL DuckDB 探测已接 | MongoDB/Redis/Kafka/ClickHouse/Oracle/SQLServer 等仍只可保存定义；受控完整导出与全部源库适配未齐 |
| 任务管理 `/tasks` | 已有配置向导、真实源表发现/映射、详情、运行/取消/计划 | 实际 Pipeline/Run，源目标组合与配置校验，引擎显式路由 | MySQL/PostgreSQL 互传继续走 ape-dts；MySQL/PostgreSQL/DuckDB 到 DuckDB 走 SwanLake，全量源快照、自动建表、分批写入与单事务替换已接 | CDC、增量位点、崩溃后远端事务核对及多节点单查询执行未完成；真实数据链仍待服务器 POC |
| 任务日志 `/integration/logs` | 专用 TaskLogsPage，按 task/run/级别/时间读取 | 实际持久化生命周期与进度摘要，受控 WebSocket | 实际运行产生日志 | 不是完整 stdout/stderr；日志保留、压缩及外部日志检索尚缺 |
| 任务运维 `/integration/operations` | 复用 OpsCenterPage | 当前同步实例、状态与调度状态读取 | 已有同步重跑/取消与调度接口 | 页面与外部调度控制台仍有复用跳转；跨实例恢复、部分写入对账与完整故障处置流程未齐 |

## 数仓设计（6）

依据：[数仓定义校验](../../crates/dts-server/src/services/product_domain/warehouse.rs)、[通用资源服务](../../crates/dts-server/src/services/product/mod.rs)、[实际引用仓储](../../crates/dts-backend-sqlite/src/repo/product_resource/helpers.rs)、[模型/前缀约束迁移](../../crates/dts-backend-sqlite/migrations/007_model_constraints.sql)。以下模块目前以控制面定义管理为主，不能称为已建成物理数仓。

| 模块 / 路径 | UI 当前实现 | 真实业务当前实现 | 运行能力 | 合同缺口 |
|---|---|---|---|---|
| 业务分类 `/warehouse/categories` | 通用树、结构化编辑与详情 | 分类资源、父级防环、排序/负责人及引用保护 | 控制面读写 | 分类迁移/批量调整与完整引用影响交互需验收 |
| 数仓分层 `/warehouse/layers` | 通用树/编辑 | 前缀、用途、顺序校验与数据库前缀唯一约束 | 控制面读写 | 分层前缀自动命名/改名影响与物理存量关联未形成完整流程 |
| 数据域 `/warehouse/domains` | 通用树/编辑 | 实际分类与负责人引用、同租户约束 | 控制面读写 | 域授权/生命周期及存量对象移动流程未齐 |
| 业务过程 `/warehouse/processes` | 通用树/编辑 | 实际数据域引用、业务粒度非空校验 | 控制面读写 | 与物理过程、模型及指标计算绑定尚未闭环 |
| 数据集市 `/warehouse/marts` | 通用树/编辑 | 分类、服务对象和负责人定义 | 控制面读写 | 集市数据建设、发布与下游消费未接 |
| 主题域 `/warehouse/subjects` | 通用树/编辑 | 实际集市引用与业务范围校验 | 控制面读写 | 主题与模型/应用完整建设和影响分析未齐 |

## 数据标准（4）

依据：[标准治理合同](standard-governance-contract.md)、[标准治理服务](../../crates/dts-server/src/services/standards/mod.rs)、[模型字段校验](../../crates/dts-server/src/services/product_domain/warehouse/model.rs)、[专用标准页面](../../web/src/pages/product/StandardsPage.tsx)。

| 模块 / 路径 | UI 当前实现 | 真实业务当前实现 | 运行能力 | 合同缺口 |
|---|---|---|---|---|
| 命名标准 `/standards/naming` | 专用标准定义/不可变修订页、实际名称校验与带原因撤回 | 实际发布/修订/撤回 CAS；模型表/列固定修订与发布闸门 | 有界纯校验、真实模型命名消费者 | 全域自动规范扫描与原型全部影响分析交互仍缺 |
| 字段标准 `/standards/fields` | 专用修订页、模型绑定/覆盖与真实采集结构检查 | 固定字段类型/空值及真实单位/枚举修订；绑定、发布门禁与检查持久化 | 实际模型消费者；标准撤回后后续执行/输出拒绝 | 当前检查范围为已采集结构；枚举内容与单位业务语义不声称已验证 |
| 枚举标准 `/standards/enumerations` | 枚举行编辑、发布修订与撤回理由 | 实际值唯一、不可改修订、字段标准固定有效版本 | 被字段标准与模型绑定链消费 | 实际数据值域扫描及自动修复尚未交付 |
| 单位标准 `/standards/units` | 专用修订页、换算参数 | 有效系数/维度、不可变版本；字段与指标固定真实有效修订 | 同维度换算、模型/指标依赖重验 | 全部引用升级和影响处置交互未齐；业务单位语义仍需内容验证 |

## 模型设计（1）

依据：[模型发布服务](../../crates/dts-server/src/services/model_publisher.rs)、[发布状态与回执](../../crates/dts-server/src/services/product/publishing.rs)、[字段编辑器](../../web/src/product/editors/ModelColumnsEditor.tsx)。

| 模块 / 路径 | UI 当前实现 | 真实业务当前实现 | 运行能力 | 合同缺口 |
|---|---|---|---|---|
| 模型设计 `/models/design` | 结构化字段/主键/DDL、实际标准绑定与覆盖/采集结构检查 | 真标准修订、物理门禁、CAS占用、真实发布回执；未知结果恢复只核对不重复建表 | 受控Flight SQL建表与精确结构/主键核对；标准撤回或校验未知保留真实结果待核对 | 实际协议6项通过，真实SwanLake尚未POC；导入、ALTER/回滚与全套模型版本交互仍缺 |

## 指标（5）

依据：[指标执行服务](../../crates/dts-server/src/services/metrics/mod.rs)、[发布冻结](../../crates/dts-server/src/services/metrics/freeze.rs)、[实际 schema](../../crates/dts-server/src/services/product/catalog.rs)。指标发布冻结真实依赖和物理证明，计算进入实际只读作业；定义校验、排队、物理执行和结果分别记录，不能混作成功。

| 模块 / 路径 | UI 当前实现 | 真实业务当前实现 | 运行能力 | 合同缺口 |
|---|---|---|---|---|
| 原子指标 `/metrics/atomic` | 专用定义/运行页、实际元数据字段选择、窗口和原生精度结果 | 实际已发布模型回执/精确物理定位/采集证明绑定；冻结字段、粒度、时间列、单位和定义版本 | 有界只读聚合作业，当前授权与依赖重验；排队不是成功 | 真实DuckDB物理执行未POC；口径变更影响、修订升级、周期调度与全套原型交互未齐 |
| 派生指标 `/metrics/derived` | 专用定义/运行页，修饰词/周期/维度选择 | 冻结原子指标、真实字段类型条件、周期窗口与依赖版本；过期依赖拒绝 | 有界只读派生聚合作业与NULL/空集区分 | 真实引擎和夏令时边界未整体POC；显式依赖升级、调度与导出流程仍缺 |
| 复合指标 `/metrics/composite` | 依赖指标编辑、专用运行与结果 | 真依赖图防环、对齐维度/窗口、原生精度快照与除零保护 | 受控真实依赖计算作业；版本变化拒绝 | 真实引擎未POC；全套经营分析、跨模型组合及变更影响尚需补齐 |
| 修饰词 `/metrics/modifiers` | 条件编辑、实际字段类型选择 | 精确元数据字段与受控操作符；冻结条件被派生计算消费 | 随派生指标编译/执行，非独立伪结果 | 独立修订发布、引用升级与原型全部条件交互未齐 |
| 时间周期 `/metrics/periods` | 长度/单位/时区配置 | 实际周期窗口编译和定义版本冻结，包含周/月边界语义 | 随派生指标编译/执行 | 夏令时真实引擎测试、日历扩展、独立修订与调度未齐 |

## 数据开发（8）

依据：[只读任务合同](read-only-job-contract.md)、[真实任务仓储](../../crates/dts-backend/src/repo/product_job.rs)、[执行 worker](../../crates/dts-server/src/services/product_jobs/runner.rs)、[结果读取](../../crates/dts-server/src/services/product_jobs/views.rs)、[临时查询 API](../../crates/dts-server/src/api/query.rs)、[结果导出](../../crates/dts-server/src/services/product_exports/mod.rs)。

| 模块 / 路径 | UI 当前实现 | 真实业务当前实现 | 运行能力 | 合同缺口 |
|---|---|---|---|---|
| 数据开发 `/development/workspace` | SQL/参数编辑、实际保存版本/历史/差异与恢复页 | 不可变修订、理由/幂等回执、真实 CAS 恢复新草稿、当前引用重验；AST 绑定与配置固定 | 有界只读 SQL worker；配置、授权和脱敏重验 | 写 SQL、目标模型落表未交付；完整目录及原型全部交互仍缺；真实引擎未 POC |
| 周期任务 `/development/periodic` | DAG 节点/依赖编辑、Cron 限制提示、计划状态卡；按状态注册/停用并显示计划版本、来源版本与待核对错误 | DAG/Cron/IANA时区校验；独立持久化计划、来源版本、授权人、理由和幂等键；HMAC绑定租户/工作流/授权人/计划版本；注册/停用未知结果不误报成功 | UTC 计划可同步 JiaScheduler；回调重新校验实际用户权限和已发布来源后进入只读 ProductJob | 社区执行器不向脚本暴露稳定 run_id，因此当前拒绝自动重试并只做秒级回调幂等；非UTC注册、人工核对专页和真实Jia/Pg/DuckDB POC仍待完成；见周期定义合同 |
| 实时任务 `/development/realtime` | 映射与流程配置工作区，运行位点只读 | 通用配置、源目标等实际引用 | CDC/start/stop/resume 未接 | 持续消费、真实位点、断点恢复、目标提交与幂等全部待实现 |
| 运维概览 `/development/overview` | 专用绿色概览，SQL/工作流来源与状态筛选、最近20条实例及详情跳转 | 读取当前租户实际 ProductJob；校验来源/分页/状态/时间，任一来源失败或响应错配时总数显示未知 | SQL文档与开发工作流实际实例；自动刷新仍由各实例页负责 | 质量/元数据/API/BI等作业仍在各自运维页，尚未形成全域汇总；没有将读取失败显示为零 |
| 工作流运维 `/development/workflow-runs` | 专用 JobRunsPage，节点结果、真实状态、重试/取消与结果导出 | ProductJobRepo 真实实例/节点/lease；详情重新授权并应用当前脱敏 | 只读作业生命周期已接 | 与周期调度、写节点、统一日志时间线及人工 reconciliation 操作的完整产品交互未齐 |
| 任务运维 `/development/task-runs` | 真实 SQL JobRunsPage、状态筛选/详情/重试/取消/受控结果导出 | 读取 sql_document 实际实例和节点，使用真实 development 权限 | 只读开发作业生命周期 | 与周期调度、统一日志时间线、全部原型筛选和人工 reconciliation 交互未齐 |
| 临时查询 `/query` | 已有 SQL 编辑与实际结果页 | tenant 引擎绑定、AST 表/字段授权、前后重验、脱敏及访问审计 | 有界真实 Flight SQL 查询入口 | 复杂 SQL 明确受限；目录/格式化/保存/受控导出的完整查询产品流程未齐；作业导出不能自动算此页导出 |
| 配置管理 `/development/configuration` | 共享/指定来源、实际对象选择、变量/NULL、原因与发布/撤回；运行参数继承/覆盖 | 迁移024不可变发布/来源绑定/作业证据，真实权限、CAS、理由/幂等和审计 | SQL/DAG 实际消费参数优先级、节点超时、配置级租约并发；撤回拒绝执行与结果出口 | 周期调度与写节点未接；当前只消费非敏感标量，不支持无实际消费者的凭据引用；生产Pg与真实引擎POC仍缺 |

## 元数据（3）

依据：[采集合同](metadata-collection-contract.md)、[血缘合同](metadata-lineage-contract.md)、[MetadataCatalogRepo](../../crates/dts-backend/src/repo/metadata_catalog.rs)、[血缘 service](../../crates/dts-server/src/services/metadata_lineage/mod.rs)、[血缘结果 UI](../../web/src/product/LineageAnalysisResult.tsx)。

| 模块 / 路径 | UI 当前实现 | 真实业务当前实现 | 运行能力 | 合同缺口 |
|---|---|---|---|---|
| 数据检索 `/metadata/search` | 通用资产目录工作区与实际字段详情 | 采集器写入的 metadata_object，版本、全字段指纹、精确引擎定位；公开写入关闭 | 读取真实采集登记，含 published/missing 状态 | 跨表字段级全文检索、完整结构版本差异、所有外部数据源覆盖尚缺 |
| 元数据分析 `/metadata/analysis` | 人工边编辑、方向/深度/容量控制、真实 SVG 图/路径/证据 | 真同租户当前元数据端点、manual actor/time/版本证据、事务内防环、强 metadata:read | 有界上下游影响遍历，stale/invalid/truncated 明确 | SQL 解析自动产边、外部采集器血缘尚未接；有界首次到达路径不是全部路径枚举 |
| 元数据采集 `/metadata/collection` | 专用 MetadataCollectionPage，定义与真实运行/取消状态 | 冻结受控 scope 与真实 information_schema；目录证明、对象反射、审计和成功提交同事务 | 绑定 SwanLake 的受控采集 worker | 当前采集范围为配置的分析端点；计划自动触发、所有外部源连接及增量变更产品流程未齐；真实引擎未 POC |

## 主数据（4）

依据：[主数据合同](master-data-contract.md)、[专用服务](../../crates/dts-server/src/services/master_data/mod.rs)、[typed 仓储合同](../../crates/dts-backend/src/repo/master_data.rs)、[记录字段编辑器](../../web/src/product/business-editors/MasterValuesEditor.tsx)。

| 模块 / 路径 | UI 当前实现 | 真实业务当前实现 | 运行能力 | 合同缺口 |
|---|---|---|---|---|
| 主数据模型 `/master-data/models` | 字段/业务键结构化编辑、校验和控制面发布 | 同一真实资源的模型版本与 schema 冻结；严格类型及 key_fields 校验 | 控制面发布，明确 physical_published=false | 未创建主数据物理表/同步下游 schema；模型升级、存量记录迁移及整套版本影响交互未齐 |
| 编码规则 `/master-data/code-rules` | 规则编辑与稳定幂等分配参数 | 真模型引用、原子序号、宽度溢出拒绝、actor/版本语义回执 | 控制面编码分配已接 | 与记录创建自动联动、规则升级迁移及所有并发环境验收尚需完成 |
| 主数据维护 `/master-data/records` | 按模型字段输入、提交/审批/驳回、修订提示 | 真业务键唯一、模型类型验证、实际非本人审批与意见、approved 修订为新 draft 保留批准快照 | 控制面记录审批已接 | 批量导入/纠错合并/完整历史对比及外部主数据消费未齐 |
| 主数据分发 `/master-data/distribution` | 映射/目标与流程配置工作区 | 通用分发定义及真引用 | distribute/retry/cancel 执行未接 | 已批准快照固定、真实下游提交、部分成功、重试幂等、取消及分发结果均待交付 |

## 数据权限（3）

依据：[授权合同](data-access-authorization.md)、[DataAccessRepo](../../crates/dts-backend/src/repo/data_access.rs)、[授权查询服务](../../crates/dts-server/src/services/data_access/query.rs)、[专用 API](../../crates/dts-server/src/api/data_access.rs)。

| 模块 / 路径 | UI 当前实现 | 真实业务当前实现 | 运行能力 | 合同缺口 |
|---|---|---|---|---|
| 我的权限 `/security/my-access` | 专用本人申请、实际对象/列、期限、提交/撤回 | 当前采集 proof 与完整字段冻结；本人 draft/rejected 编辑；提交与撤回原子维护实际授权 | 授权进入只读查询、作业、质量、API/BI 受控出口 | 权限产品全流程与跨模块体验尚需集中验收；当前查询语法范围受限，不能声明覆盖任意 SQL |
| 权限审批 `/security/approvals` | 专用审批队列、真实意见/期限/幂等请求 | 精确 data_security:approve、非本人审批、同租户实际用户、批准/驳回/撤销及不可改回执 | 实际 grant 生效/撤销；返回前重新核验原 grant | 不保证撤销物理中止已经进入远端的查询；统一待办、通知与更细授权委派范围未齐 |
| 权限审计 `/security/access-audit` | 专用只读真实审计列表 | 实际变更与受控查询事件，hash/元数据/列/结果，不存 SQL/敏感值 | 业务流程产生日志 | 完整多条件时间检索、受控全窗口导出、归档/保留及所有外部访问埋点未齐 |

## 敏感数据（4）

依据：[识别执行与发现合同](sensitive-recognition-contract.md)、[识别服务](../../crates/dts-server/src/services/sensitive_recognition/mod.rs)、[真实发现仓储](../../crates/dts-backend-sqlite/src/repo/sensitive_findings/mod.rs)。当前已有有界取样识别执行器与原子结果生产链，尚未完成真实引擎POC。

| 模块 / 路径 | UI 当前实现 | 真实业务当前实现 | 运行能力 | 合同缺口 |
|---|---|---|---|---|
| 数据密级 `/security/classifications` | 定义/校验/发布/撤回 | 真等级与处理要求校验；被规则及任务快照冻结和重验 | 识别链实际消费已发布定义 | 自动变更全域密级、分类建议直接生成强制脱敏策略尚未交付 |
| 识别规则 `/security/recognition-rules` | 结构化匹配/来源类型与发布动作 | 有界Rust正则/匹配模式、成本校验、字段类型与密级版本绑定 | 被真实识别任务消费 | 全量自动安全治理与所有来源适配未齐；样本命中不等同全表结论 |
| 识别任务 `/security/recognition-tasks` | 专用配置/真实运行/发现复核三页签 | 真实元数据定位、列范围、有限取样、独立读取授权及lease/fence/cancel | 实际scanner作业；节点结果和发现同事务提交；不保存源样本 | 真实DuckDB未POC；自动调度、外部源覆盖、后续分类策略应用仍缺 |
| 数据发现 `/security/discovery` | 专用筛选、详情、确认/驳回与版本冲突反馈 | 真任务/节点/列产生findings，保留计数/定义hash/范围；actor/reason/CAS/幂等复核 | 可信producer与review状态机已接 | 复核不自动改源数据或授权/脱敏；完整策略联动、批量处置及原型全部交互未齐 |

## 数据脱敏（2）

依据：[输出脱敏合同](query-output-masking.md)、[MaskingService](../../crates/dts-server/src/services/data_masking/mod.rs)、[结果重验与收据](../../crates/dts-server/src/services/data_masking/output.rs)。

| 模块 / 路径 | UI 当前实现 | 真实业务当前实现 | 运行能力 | 合同缺口 |
|---|---|---|---|---|
| 脱敏规则 `/security/masking-rules` | 结构化配置、真实预览、发布/撤回 | 真 metadata/column/group，规则参数严格校验、当前策略 hash 与输出收据 | 查询、作业结果、API、BI 与作业导出出口接遮盖/HMAC/部分保留，并重验 | 当前只覆盖支持的单表/字段表达式；复杂派生/筛选明确拒绝；规则范围和所有出口仍需 POC，不是任意 SQL 脱敏平台 |
| 用户组 `/security/user-groups` | 实际用户多选、成员动作、发布/撤回 | 同租户真实活跃 User 成员，脱敏适用组与真实版本 | 输出策略消费实际组成员 | 组治理、停用/删除受引用影响及整套成员变更交互需验收；不替代数据 grant 或账号状态 |

## 数据水印（2）

依据：[水印目标合同](full-scope-contract.md)、[导出实现](../../crates/dts-server/src/services/product_exports/encode.rs)、[脱敏收据](../../crates/dts-server/src/services/data_masking/types.rs)。已有脱敏结果签名收据不等同于这两个水印业务模块。

| 模块 / 路径 | UI 当前实现 | 真实业务当前实现 | 运行能力 | 合同缺口 |
|---|---|---|---|---|
| 水印溯源 `/security/watermark-tracing` | 通用审核/文件摘要属性工作区 | 本人资源范围约束，无实际溯源处理器 | trace 未接 | 文件标记提取、清单校验、真实匹配/不匹配/无法验证及证据产出待实现 |
| 水印设置 `/security/watermark-settings` | 通用策略编辑器 | 策略定义保存与字段引用基础 | 真实导出植入/签名清单未接 | 策略生效、来源列/清单输出、密钥轮换和溯源链待实现；不声称隐形抗篡改算法 |

## 数据质量（6）

依据：[质量执行合同](data-quality-execution.md)、[质量服务](../../crates/dts-server/src/services/data_quality/mod.rs)、[规则编译器](../../crates/dts-server/src/services/data_quality/compiler.rs)、[报告](../../crates/dts-server/src/services/data_quality/reporting.rs)、[专用页面](../../web/src/pages/product/QualityReportPage.tsx)。

| 模块 / 路径 | UI 当前实现 | 真实业务当前实现 | 运行能力 | 合同缺口 |
|---|---|---|---|---|
| 质量监控 `/quality/monitoring` | 专用定义、观察、告警处理与本人提醒页 | 真实质量运行窗口/采集证明；同事务持久化观察与open/acknowledged/resolved，真实后续恢复；无结果保留告警 | 手动评估、处理确认、真实恢复和本人提醒读取已接 | 自动监控调度、邮件/短信/webhook外发尚未接；pending提醒不等同已投递 |
| 质量对象 `/quality/objects` | 真 metadata 引用与字段属性 | 当前采集证明、字段/表定位与版本校验 | 供质量执行消费 | 所有 source drift 详情与完整负责人/影响交互需验收 |
| 质量规则 `/quality/rules` | 非空/唯一/值域等结构化编辑与 validate | 固定规则编译、类型/条件/成本边界与版本 | 真实 COUNT/窗口检查 SQL | 目标 suggest/AI 草稿尚未接；任意用户表达式/样本明细不是已支持能力 |
| 质量任务 `/quality/tasks` | 规则/对象/样本上限配置与提交 | 冻结任务/对象/规则/元数据、真实排队和节点状态 | 只读 worker 执行真实检查、取消、权限重验 | 自动计划触发与完整重试/运维交互未齐；真实源库质量 POC 未运行 |
| 校验结果 `/quality/results` | 专用 JobRunsPage quality scope、真实节点计数与错误 | 执行结果 typed 投影，执行错误/数据失败/空数据分别表示 | 实际作业节点生成 | 尚无独立全规则分页/样本证据产品页；不提供伪造人工成功结果 |
| 质量报告 `/quality/reports` | 专用任务/时间窗口生成与真实报告 | 最多 2000 作业汇总，来源版本、空窗口/截断/比例明确 | 即时真实汇总 | 报告独立持久化快照、自动周期生成/发布、正式报告导出尚未交付 |

## 数据资产（3）

依据：[资产合同](assets-contract.md)、[AssetsRepo](../../crates/dts-backend/src/repo/assets.rs)、[资产服务](../../crates/dts-server/src/services/assets/mod.rs)、[图实现](../../crates/dts-server/src/services/assets/graph.rs)、[用量页面](../../web/src/pages/product/AssetUsagePage.tsx)。014 和新 UI 已落源码，本次审计错误修复后的测试仍待主代理复验。

| 模块 / 路径 | UI 当前实现 | 真实业务当前实现 | 运行能力 | 合同缺口 |
|---|---|---|---|---|
| 资产地图 `/assets/map` | 真资产多选、保存布局、真实 SVG 图/证据 | 仅真实 selected assets；已存引用与当前人工血缘投影；节点源权限和 stale/invalid/cap 明确 | 有界实际关系分析 | 自动发现全部资产关系、自动 SQL 血缘、多层大图与完整布局交互未齐；不是全企业关系扫描 |
| 资产目录 `/assets/catalog` | 来源种类动态对象 picker、目录卡片、发布有效/失效详情 | 真对象/owner/domain；draft→registered→published→withdrawn；冻结来源版本/hash/proof/实际 actor/time | 目录定义发布，明确 physical_published=false | 目录发布不执行物理数据、指标计算或分发；完整资产授权/经营生命周期及全套交互尚需验收 |
| 资产分析 `/assets/analysis` | 专用时间窗口与统计结果，未知/超量明确 | 对真实 metadata ID 读取 data_access query 审计；最多 5000 事件，超额不输出伪完整统计 | 已记录事件统计 | 仅受控查询审计来源，其他来源为 not_instrumented；引用统计趋势、API/外部访问接入与分析导出未齐 |

## 数据服务（4）

依据：[数据 API 合同](data-api-contract.md)、[DataApiService](../../crates/dts-server/src/services/data_services/mod.rs)、[ServiceAccessService](../../crates/dts-server/src/services/service_access/mod.rs)、[真实仓储合同](../../crates/dts-backend/src/repo/service_access.rs)、[鉴权 API](../../crates/dts-server/src/api/service_access.rs)。015/016 与本轮桥接为新增源码，尚不能声称编译、事务测试或真实调用已经通过。

| 模块 / 路径 | UI 当前实现 | 真实业务当前实现 | 运行能力 | 合同缺口 |
|---|---|---|---|---|
| API 管理 `/services/apis` | 专用定义/schema/参数与实际运行结果页 | 固定单表明细查询、参数 AST 绑定、响应 schema/发布版本冻结与 withdraw | 内部只读 job；外部凭据调用执行链源码已接 | 真实 HTTP/引擎链未 POC；多表/复杂 SQL 与任意 API 编排明确未支持；完整 API 文档/版本升级/兼容流程未齐 |
| 服务目录 `/services/catalog` | 专用订阅与审批页 | 真 app owner、实际 published API/version；申请/非本人 approve/reject/revoke，稳定幂等回执与到期约束 | 实际授权被调用入口消费；新增待验证 | 目录浏览/订阅全流程和自动通知需验收；过期 approved 授权需显式 revoke，不会偷偷换新版本 |
| 我的调用 `/services/calls` | 专用真实调用/拒绝列表 | 完整许可记录及独立已认证拒绝记录；不造缺失 subscription；pending/未知事实状态 | 实际调用和配额拒绝产生日志；新增待验证 | 日期/状态多维检索与导出未齐；调用量覆盖仅该真实入口，不是全部企业 API 流量 |
| 应用管理 `/services/applications` | 专用 owner/来源/期限与凭据 panel；secret 仅首次展示 | 真 User owner、一次凭据签发、摘要存储、撤销/到期、历史 FK、owner 转交限制 | 真实凭据、origin 策略、每 API 与每凭据双分钟配额/429源码已接 | 新增015未验收；全套轮换/批量撤销/归档交互需复验；不得把页面有凭据列表视作外部链已测通 |

## BI（1）

依据：[BI 合同](bi-report-contract.md)、[报表服务](../../crates/dts-server/src/services/bi_report/mod.rs)、[共享服务](../../crates/dts-server/src/services/bi_share/mod.rs)、[共享仓储](../../crates/dts-backend/src/repo/bi_share.rs)、[BI 页面](../../web/src/pages/product/BiReportPage.tsx)。017 共享功能为新增源码，未验收。

| 模块 / 路径 | UI 当前实现 | 真实业务当前实现 | 运行能力 | 合同缺口 |
|---|---|---|---|---|
| 人员 BI 分析 `/bi/personnel` | 结构化维度/度量/图表，真实柱/线/饼/表结果、作业导出；共享页面 | 实际单表聚合、版本冻结、当前授权/脱敏；同租户真实收件人、到期/撤销共享，打开仍按当前 reader 授权 | 只读聚合 worker；成功结果 JSON/CSV 导出；共享链源码已接 | 人员模型专属分析、联表/钻取/仪表盘完整流程未齐；共享017未验证；真实数据图表和权限链需 POC，无匿名分享授权 |

## 产品支撑（2）

依据：[支撑目录](../../web/src/product/assets-service-catalog.ts)、[目标合同](full-scope-contract.md)、[标签绑定合同](tag-assignment-contract.md)、[统一告警策略合同](alert-policy-contract.md)、[旧告警服务](../../crates/dts-server/src/services/alert_svc.rs)。

| 模块 / 路径 | UI 当前实现 | 真实业务当前实现 | 运行能力 | 合同缺口 |
|---|---|---|---|---|
| 标签管理 `/support/tags` | 绿色编辑工作区、颜色/完整分组范围、真实目标选择、活动绑定清单与逐项解绑；只读用户无写操作 | 标签定义；同租户真实多类资源绑定；范围/版本/权限校验；幂等不可变回执、审计原子提交和活动绑定删除保护；SQLite/PostgreSQL 仓储 | 定义、绑定与解绑源码已接，本轮 SQLite 4 项仓储测试和前端 3 项交互测试通过 | 批量导入、继承/规则自动打标、各业务目录按正式标签筛选和整体 POC 未完成；历史字符串 tags 不是正式关系 |
| 告警设置 `/support/alerts` | 绿色策略编辑、启用/窗口/阈值/冷却/自动评估间隔、真实用户与模板选择、含渠道/尝试/错误的评估投递历史；站内告警进入统一消息页 | 服务端只读真实任务失败/质量超阈值/近期健康异常；版本/权限/活动收件人/模板变量校验；不可变幂等评估、事务审计、冷却抑制；持久化周期游标、调度与投递租约/fence、最大尝试终态和站内已读审计；SQLite/PostgreSQL仓储 | 手工与自动评估、站内投递和统一收件箱源码已接；SQLite告警7项、消息回归9项、前端相关14项通过；外部渠道缺少驱动时明确失败 | 邮件/短信/企微/钉钉/Webhook真实驱动与凭证治理仍待交付；系统健康只使用已有探测记录，不是实时主动探测；整体POC未完成 |

## 公共权限与系统（20）

依据：[Sa 会话与认证](../../crates/dts-server/src/services/sa_auth.rs)、[本人资料合同](profile-and-run-log-contract.md)、[系统 API](../../crates/dts-server/src/api/system.rs)、[真实用户服务](../../crates/dts-server/src/services/user_svc.rs)、[组织/菜单/模板校验](../../crates/dts-server/src/services/product_domain/access.rs)、[内容可见性](../../crates/dts-server/src/services/system_content.rs)、[站内消息](../../crates/dts-server/src/services/message_delivery.rs)、[门户 UI](../../web/src/pages/product/PortalWidgetView.tsx)。

| 模块 / 路径 | UI 当前实现 | 真实业务当前实现 | 运行能力 | 合同缺口 |
|---|---|---|---|---|
| 个人中心 `/account/profile` | 专用本人资料、旧密码/新密码、会话撤销 | 真 User scoped 更新和密码 CAS，真实 Sa 会话/凭证指纹失效 | 控制面账户流程已接 | 注册、找回密码、一次性验证渠道及原型认证扩展未齐；密码策略目前为代码策略，未消费基础设置 |
| 待办事项 `/account/work-items` | 专用真实来源/分页与业务入口 | 实际三类审批排除本人提交、本人queued/reconciliation运行和未读消息；逐源权限/known/unknown/restricted | 从真实生产者投影，无人工待办/伪完成 | 独立领取/截止策略、统一告警和全部业务入口参数交互未齐；新后端待统一回归 |
| 系统统计 `/system/statistics` | 专用UTC周期与来源卡片、重读后JSON导出 | 真账户/数据源/任务/资源及同步/产品运行计数，定义和执行口径分开 | 按实际仓储来源聚合，失败/无权为null | 来源逐个观测非全局快照；跨源趋势/保留/原型全部图表未齐；新后端待统一回归 |
| 使用统计 `/system/usage` | 专用UTC周期、本人来源与导出 | 本人已持久化登录/资源操作/查询访问/服务调用/消息记录，调用状态分别计数 | 当前身份本人投影，失败不显示零 | 无法归属的登录失败不推断本人；全出口埋点与经营趋势尚缺；新后端待统一回归 |
| 租户管理 `/system/tenants` | 复用系统 tenant tab | 真租户状态与账号隔离；专门 tenant:create 可创建租户/初始角色/管理员 | 控制面实际管理 | 列表仍仅当前租户；到期/菜单模板分配、跨租户平台管理与原子完整入驻流程未齐 |
| 菜单模板 `/system/menu-templates` | 专用定义/当前分配/历史页；实际导航消费 | 冻结真实已注册菜单树/标题/顺序/权限；租户分配/撤销CAS、幂等、到期和历史 | 当前后端导航策略实际决定Sidebar与可见页面，服务授权独立 | 跨租户平台管理员工作区、版本差异比较与全部模板交互未齐 |
| 组织管理 `/system/organizations` | 真实组织树/成员/负责人/迁移交接及历史专页 | 独立真实membership单归属、版本墓碑、同租户active人/组织、manager必须实际成员；CAS/交接/审计 | 专用事务迁入/迁出/负责人更新；公告读取本人真实归属；旧props不作为关系事实 | 不自动赋RBAC或推断父级成员；组织数据权限策略、批量导入和用户侧全部关系交互仍缺；新后端待统一回归 |
| 用户管理 `/system/users` | 复用真实系统 users tab | 账号、状态、角色、密码重置、越权防护、会话撤销 | 实际用户和权限流程 | 组织/组关系仍主要在各域维护，未形成完整用户侧关系管理；用户生命周期与原型流程需验收 |
| 角色管理 `/system/roles` | 复用真实系统 roles tab | 真 role/permission/member，系统角色保护与授予范围约束 | 实际权限变更；有引用删除事务保护 | 独立角色启停状态及原型全部成员管理/影响交互未齐 |
| 权限列表 `/system/permissions` | 系统 canonical permissions tab | 从真实注册权限列表展示，与实际角色绑定 | 认证/业务 API实际消费权限 | 新域细粒度权限仍需随实现扩展并验收；不能用菜单隐藏替代后端授权 |
| 菜单管理 `/system/menus` | 树、路径/权限/顺序编辑；SideBar读取实际策略 | 仅已注册产品路径/权限，真实父级防环；模板分配时冻结菜单树 | 导航实际消费分配快照；后台重验失败/到期关闭旧策略 | 修改菜单不隐式改已分配快照，需要显式重分配；完整版本比较及全部交互未齐 |
| 字典管理 `/system/dictionaries` | 结构化枚举行编辑 | scope、code/value 等实际定义校验与基础引用保护 | 定义管理 | 业务消费字典、停用约束和修订影响流程未齐 |
| 公告管理 `/system/announcements` | 专用管理/已发布内容与安全纯文本展示 | 发布/撤回，实际用户/角色/组织受众、publish_at/expires_at可见性 | 按时间与真实受众读取 | 计划为读取时生效，没有独立投递/阅读回执；完整定时发布/通知和产品流程尚需验收 |
| 登录日志 `/system/logs/login` | 通用只读 monitor 列表 | 登录流程产生实际成功/失败/IP事件，无密码/令牌 | 认证时产生日志 | 当前失败原因较粗；完整时间/来源检索、受控全量导出与保留策略未齐 |
| 操作日志 `/system/logs/operations` | 通用只读 monitor 列表 | product_resource 写事务产生 actor/资源/前后安全摘要与 domain action | 与资源写同事务，禁止人工伪造 | 当前该事件 API只支持分页；不是所有旧业务写 API的统一审计，时间/对象筛选与归档/全量导出未齐 |
| 系统监控 `/system/monitoring` | 当前复用 OpsCenterPage | 读取已有任务/调度范围 | 已有状态/实例入口 | 缺统一服务/引擎/数据库健康、worker资源及新增域健康指标；外链控制台不等同平台内完整监控 |
| 消息管理 `/system/messages` | 专用发件、真实收件/未读、详情与幂等发送 | 站内实际收件行/批次、真实活跃发送/收件人、只给本人 mark_read | 站内消息事务投递 | 邮件/短信/企微/钉钉/webhook 当前该产品服务明确未配置；外发、真实失败重试/回执未完成 |
| 消息模板 `/system/message-templates` | 结构化变量、validate/preview | 模板参数严格检查、安全纯文本替换，站内发送实际消费 | 真预览；预览不投递 | 各外部渠道模板/版本/投递适配未齐 |
| 基础设置 `/system/settings` | 通用品牌/策略配置编辑 | 配置资源保存、schema字段约束 | apply/领域 validate 未接 | 品牌/登录/密码/通知策略尚未进入实际认证或渠道消费；当前保存不表示策略已生效 |
| 门户设计 `/system/portals` | 布局/主题结构化编辑，专用预览/可见页；真实卡片/图/公告/快捷入口 | 受控组件、真角色授权、发布/撤回，内容服务按角色可见；组件再查模块权限 | 真实受控组件读取，图明确最多5条当前记录状态分布 | 独立 authorize 流程与全部组件/布局交互未齐；当前小窗口图不能当完整历史统计；跨原型全部门户交互尚未还原 |

## 横向缺口与下一轮交付边界

1. **DuckDB 数据入口与写任务**：MySQL/PostgreSQL/DuckDB 到 DuckDB 的 SwanLake 全量链路源码已接入源端快照、自动建表、批次进度、取消和单事务回滚。服务器真实 POC、CDC、增量位点、崩溃后未知提交核对与跨端点 DuckDB 搬运仍是核心交付边界。
2. **调度与实例统一**：已有 JiaScheduler 面向同步任务的计划接口和只读 ProductJob lease/fence。周期开发、质量、采集等定义的计划尚未统一接入；运行、节点、日志、取消、重试与未知副作用处置需在产品中统一表达。多 worker 领取不等于分布式 SQL。
3. **指标与主数据分发**：指标已接实际模型/元数据证明、冻结发布、有界计算与结果重验，真实引擎POC、依赖显式升级与调度仍缺；主数据完成控制面模型/编码/审批源码，尚无下游提交及分发实例。
4. **安全与结果闭环**：数据授权、输出脱敏、取样识别与发现复核已有实际入口和重验。水印、全局安全策略以及全部数据出口覆盖仍缺；取样命中不是全表结论，受控导出与脱敏收据不能替代水印业务。
5. **公共后台消费链**：真实组织成员/负责人交接、菜单分配/撤销/到期、本人待办及周期统计已新增实际消费者。系统策略apply、字典全域消费和跨租户运营仍缺。消息当前只有站内真实投递，统一告警和外发适配继续实现。
6. **产品交互完整度**：通用工作区、专用页面和部分真实结果展示已落地，仍需要逐流程补齐导入、版本对比、发布/撤回、批量管理、筛选、错误恢复及操作反馈。206画布还需继续按可恢复证据核对，不能宣称完整复刻。

以上是源码事实盘点，不是商业化可用性认证。继续交付应逐域满足 [完整功能合同](full-scope-contract.md)，同时记录定义管理、真实控制面状态机、物理执行和UI流程的独立证据，避免用统一 CRUD 覆盖全部模块验收。

## 验证快照

以下记录主代理于 2026-10-09 串行执行的实际结果。整体 POC 与前后端服务未启动；后续模块的新增源代码不自动继承这些验证结果。

| 范围 | 已知结果 | 不能推导出的结论 |
|---|---|---|
| dts-server lib 最近批次 | 327项全部通过；022标准/023开发修订已注册，包含组织/投影/质量、精确BIGINT与Windows密钥ACL回归 | 不代表真实数据平面、Pg运行或全平台产品完成；024新增服务另列验证 |
| SQLite assets 本轮8项 | 8 项全部通过；登记/发布审计 CHECK 修复已复验 | 未验证 PostgreSQL 资产运行及真实数仓全链路 |
| 前端构建与既有专项 | 上批TypeScript/Vite构建通过，主入口617.77kB；配置8/真实交互12共20项、任务运维实际路由/Sidebar共33项通过，产品目录合同检查通过；此前配置/精度/业务编辑/真实交互27项通过 | 不代表全部206画布已还原或验收；尚未浏览器视觉验收；周期字段新增构建结果另列 |
| 服务集成最近批次 | BI分享3、数据权限API1、数据API6、模型物理发布协议6、开发作业10、产品生命周期HTTP2、敏感识别4项通过；外部调用6项已全部复验通过，统一脱敏JSON拒绝伪造身份/SQL | BI使用真实申请/非本人审批grant；尚未做真实Pg/DuckDB整体POC |
| 敏感发现018、租户导航019 | SQLite新增23项全通过（敏感发现5、导航8、调用10）；覆盖真实提交/回滚/fence/撤销/复核与分配幂等 | 不代表真实引擎扫描、所有导航交互或生产Pg已验证 |
| SQLite组织020、质量告警021与投影 | 最新36项通过（驱动lib27、真实投影6、质量告警3）；覆盖真实成员交接、权限与幂等、来源统计、观察/确认/实际恢复和收件 | 不是生产Pg或真实数仓全链路；自动监控调度和外部投递未包含 |
| 022/023与界面专项 | SQLite lib38/标准9/质量3共50项通过；标准页/模型绑定13、开发修订10、查询8、路由/编辑/工作区25专项通过 | 不代表浏览器视觉或整体POC；标准内容语义与物理引擎尚未整体验收 |
| 024运行配置当前批次 | SQLite配置5项通过（含审计失败回滚/哈希证据）；此前lib38/作业9通过；服务执行4项通过；产品生命周期HTTP最新3项全部通过（含配置纯校验/权限/严格参数/回执重放） | Pg仓储和迁移已接入并完成全特性编译，尚未验证生产事务与真实DuckDB；整体POC仍未开展 |
| 周期定义与开发运维最新批次 | 调度客户端库6/社区协议6、服务端库329、开发作业11、产品生命周期4项通过；前端周期/运维/真实交互/Sidebar共37项通过，运维实际路由专项2项复验通过；构建618.02kB、lint、83模块目录检查及离线全工作区全feature `cargo check`通过 | 周期定义校验不等于自动调度注册；非UTC社区适配未实现，真实Jia/Pg/DuckDB及浏览器POC尚未开展；未因本机链接成本重跑全部集成测试矩阵 |
| 周期计划控制面本批次 | dts-server离线编译通过；计划状态机单元2项、前端周期/工作流8项通过；前端生产构建618.51kB及lint通过；目录仍为19组83模块 | 真实JiaScheduler没有启动，注册/停用远端协议、Pg事务、秒级幂等边界和浏览器视觉尚未POC；不代表自动重试或非UTC计划可用 |
| 标签与统一告警当前批次 | 标签SQLite 4/服务范围1/前端3项通过；统一告警SQLite 4/字段边界1/前端2项通过；默认SQLite与`postgres,engine-ape`离线编译通过；最新前端构建619.60kB、lint和19组83模块目录检查通过 | 告警外部投递worker、自动周期评估、生产Pg事务与浏览器视觉未POC；标签自动打标和跨目录消费未完成 |
| 三个上游源码同步 | Ape-DTS `main`=`b6985c14`、JiaScheduler `main`=`7b232d8d`、SwanLake `main`=`0f04b10e`，均核对官方GitHub upstream；SwanLake `dev`已合并社区main并保留脱敏改造 | 尚未编译三个完整上游仓库，也未做版本升级兼容矩阵；源码最新不等于稳定生产版本，正式构建仍需固定并验证SHA |
| 数据平面及整体POC | 尚未开展本轮整体POC | 尚无真实源库→DuckDB→模型/指标/质量→资产/API/BI的完整结果与故障恢复证明 |

已有模块合同中的历史定向测试结果属于其各自验证范围。新领域落盘后必须重新确认相应编译和定向边界，不能把旧批次通过数累加成全平台完成率。最终 POC 留在功能链路完成后集中进行；开发阶段仍保留必要静态、编译和领域回归验证。
