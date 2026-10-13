# DTS Platform — 领域术语表

> 本文档定义 DTS Platform 的核心领域概念，所有代码、API、文档、AI prompt 应统一使用以下术语。

## 核心实体

### Pipeline（数据管道）
**定义**: 一个数据同步管道的定义，描述"从哪来、到哪去、怎么同步"。
**数据库**: `sync_task` 表
**别名历史**: Task, SyncTask, 同步任务, 任务
**示例**: "将 MySQL source_db.customers 全量同步到 PostgreSQL target.public.customers"

### PipelineRun（管道运行）
**定义**: Pipeline 的一次具体执行实例。
**数据库**: `task_run` 表
**别名历史**: TaskRun, Run, Execution, 运行记录
**关键属性**: run_id, status, start_time, end_time, rows_synced, error_message
**生命周期**: Running → Completed / Failed / Cancelled

### Schedule（调度规则）
**定义**: Pipeline 的定时触发配置。
**别名历史**: cron, cron_expr, 定时任务
**关键属性**: cron 表达式、是否启用

### Datasource（数据源）
**定义**: 外部数据库的连接配置。
**数据库**: `datasource` 表
**关键属性**: db_type (mysql/postgresql/sqlite), host, port, database_name

### Tenant（租户）
**定义**: 多租户隔离的基本单位。所有 Pipeline、Datasource、Alert 归属于某个 Tenant。
**数据库**: `tenant` 表

### AlertRule（告警规则）
**定义**: 对 Pipeline 运行指标的监控阈值。
**监控指标**: sync_delay, error_count, row_diff, bytes_diff
**运算符**: gt, gte, lt, lte, eq

## AI 分析实体

### Diagnosis（故障诊断）
**定义**: 对特定 PipelineRun 失败原因的分析。
**API**: `POST /api/v1/ai/diagnose?task_id=X`
**输入**: DiagnosisContext（pipeline 名称、源/目标 DB、错误日志、最近运行历史）
**输出**: 根因分析 + 影响评估 + 修复建议 + 预防措施

### AnalysisReport（分析报告）
**定义**: 对 Pipeline 整体运行趋势的 AI 生成报告。
**API**: `POST /api/v1/ai/report?task_id=X`
**输入**: ReportContext（成功率、总行数、趋势数据）
**输出**: Markdown 格式报告（执行摘要、趋势分析、异常检测、优化建议）

### NL2SQL（自然语言转 SQL）
**定义**: 用户通过自然语言描述数据需求，AI 生成对应的 DuckDB SQL。
**API**: 复用 `POST /api/v1/ai/chat`

## 分析引擎

### SwanLake
**定义**: 外部分析引擎，基于 DuckDB + Arrow Flight SQL 协议。
**连接**: gRPC on port 4214 (Flight SQL), HTTP on port 4215 (Status)
**用途**: 执行用户提交的 SQL 查询，通过 PostgreSQL ATTACH 读取 DTS 数据

## 架构决策

- Pipeline 的代码命名 (`SyncTask`, `TaskRun`) 保持向后兼容，不强制改名
- 前端用户界面继续使用"任务"作为 Pipeline 的中文显示名（用户习惯）
- 新增代码（AI prompt、API 文档）应使用统一术语
- DB 表名保持现状（`sync_task` / `task_run`），不做迁移
