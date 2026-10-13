# 个人待办与平台统计实施合同

这是本产品的实际实施合同，不声称从购买原型恢复了精确内部数据结构。待办和统计是现有业务生产者的只读投影，不允许客户端创建 `work_item`、`system_statistics`、`usage_statistics` 来替代真实结果。

## 接口与授权

`GET /api/v1/product/work-items?source=<enum>&limit=20&offset=0`：任何有效平台用户可进入，身份仅来自 Sa 会话。`source` 可省略，取六个独立来源；指定时仅返回该来源。limit 1..100、offset 0..100000。每个来源分别分页，不将六种工作流混成一个可“完成”的队列。

`GET /api/v1/product/statistics?view=system|usage&start=<RFC3339>&end=<RFC3339>`：system 要求真实 `system:read`；usage 为本人统计。所有时间转换 UTC，使用 `[start,end)` 半开窗口，正数且最多366天，结束时间不得超出现时5分钟。非法枚举、参数、时间窗应400，不冒充来源读取失败。

所有仓储来源先验证租户和真实用户处于 active，并从实际 role_permission 读取权限。应用在每个来源读取前后复核当前身份/完整权限集合；读取期间权限变化，整个响应拒绝，不能释放旧结果。仓储来源与权限在同一只读事务内读取；SQLite 使用事务快照，Pg 使用 REPEATABLE READ READ ONLY。来源之间并非全局一致性快照，响应对此明确说明。

## 待办真实生产者

| source | 实际来源 | 条件与权限 | 来源操作 |
| --- | --- | --- | --- |
| data_access_approvals | data_access_request JOIN product_resource | 同租户 pending，requester 非本人，data_security:read + approve | 进入数据权限审批页，原有 typed CAS 审批消费该待办 |
| master_data_approvals | master_record_identity JOIN product_resource | 同租户 pending，实际 submitted_by 非本人且非NULL，master_data:read + approve | 进入主数据记录页；批准/驳回由真实修订事务完成 |
| service_approvals | service_subscription JOIN product_resource | typed/资源 pending 与版本一致，owner/requester 均非本人，services:read + approve | 进入服务订阅页，原有 typed 订阅审批消费该待办 |
| queued_jobs | product_job JOIN product_resource | 本租户本人已入队，来源领域 read + execute | 进入来源运行页；表示等待工作器，不等于用户手工执行或已成功 |
| reconciliation_jobs | 同上 | 本租户本人 reconciliation_required，来源领域 read + execute | 核对实际运行后使用既有受控 reconcile 流程，不自动重放非幂等写入 |
| unread_messages | message_delivery | 本租户本人 recipient，read_at NULL | 进入本人消息收件箱，真实已读回执后待办消失 |

输出仅来源标识、版本、标题、状态、时间、期限与合法产品路径。待办不返回消息正文、审批数据值、查询结果、凭据、密码、令牌、任意跳转 URL。角色审批队列没有捏造个人 assignee/claim；V1 不建立独立人工任务池。没有同步运行生产者的草稿定义也不会被凭空当作“待执行”。

## 真实统计口径

所有数量由来源表实际 `COUNT(*) GROUP BY` 产生，过滤参数有界绑定，客户端不能提交 SQL/表名。系统页定义/账户/数据源按创建窗口分组并使用当前保存状态，不表示历史时点状态；运行按实际开始/创建时间过滤。

| source | 观测口径 |
| --- | --- |
| users | 本租户非deleted账户创建窗口；需 user:manage |
| datasources | 本租户数据源配置创建窗口；需 datasource:read；health_status 是最后保存探测状态，不声称当前实时健康 |
| sync_definitions | 本租户 sync_task 创建窗口、当前定义状态；需 task:read；不是执行次数 |
| product_definitions | 当前有领域read权限的产品配置/业务目录记录，排除登录、操作、发现、质量结果等只读投影；按kind/status分组；不是执行次数 |
| sync_runs | task_run 通过真实 sync_task 绑定租户，start_time 窗口，实际保存运行状态；需 task:read |
| product_jobs | 本人 product_job 创建窗口，当前来源领域read，按 source_kind/status；queued、failed、cancelled、reconciliation_required 分开 |
| login_events | product_resource(kind=login_event) 中真实 created_by=本人、创建窗口事件；当前认证入口失败事件往往未绑定 user_id，不将 unauthenticated 事件虚构归属本人 |
| resource_operations | product_resource_audit 中本人持久化的资源写操作，排除 login_event 创建审计；不声称覆盖平台全部操作 |
| query_access | 本人 data_access_audit(action=query) 窗口，真实 succeeded/failed/denied；需 data_security:read；不是引擎扫描行数或性能估算 |
| service_calls | 本人作为owner的真实 service_call 预留窗口，按实际终态；需 services:read；pending 只是已预留，不是成功，不把预留前拒绝当作已执行调用 |
| inbox | 本人实际 message_delivery 投递窗口，当前已读/未读状态 |

外部引擎库存、机器负载、实际计算吞吐、未持久化事件的完整性不在这些计数的观测范围。已有运维页的数据不通过配置数量猜估。统计来源读取独立，数据库变化可能导致相邻卡片属于不同观测瞬间。

## 结果状态与导出

每个来源有 `observation {source,source_table,observed_at,state,total,filter,error_code}`。

- known：只有来源真实查询成功才给 total，可真实为0。
- unknown：来源读取/解码/计数失败，total=NULL、data=[]，稳定错误码不泄露原始数据库错误。
- restricted：当前来源权限不足，total=NULL、data=[]。不得显示成成功或0。

顶层绑定实际 tenant_id/actor_id，详情仅返回已授权的来源记录。前端校验身份、来源集合、UTC窗口、分页、计数与状态，权限/会话改变时用新的查询键并重挂载工作区，不保留旧占位结果。刷新被拒后旧列表/计数立即隐藏。

统计JSON导出先重新读取同一已应用窗口，再复核当前会话与前端权限范围，仅导出该次真实投影；来源 unknown/restricted 会保留明确状态，不填0。导出请求失败不下载旧缓存。本轮没有独立“统计结果”伪生产者、匿名分享、外部表扫描、未实现的CSV/Excel导出按钮。

## 接线与验证

Root接线：shared repo模块；SQLite/Pg repo导出与BackendContext；services/platform_projection；构造 `PlatformProjectionService::new(repo,auth)`；注册新 api/platform_projection 的 `work_items`/`statistics`；三个目录模块分别选 WorkItemsPage、PlatformStatisticsPage(system/usage)。未增加迁移，不占用018..021编号。

工作待办/本人usage/消息收件入口必须对有效用户可用，管理目录功能仍在自己的服务端领域权限下校验；导航许可不能替代当前来源授权。

测试源覆盖真实 ProductJob 入队、MessageDelivery 投递/已读、MasterData 提交/批准、DataAccess 提交/驳回，以及租户隔离、角色撤销、窗口边界、缺失来源错误。服务pure测试区分unknown/restricted/known0。前端定向测试覆盖缓存隔离、刷新拒绝、本人入口、来源分页、计数口径、非法窗口和导出失败。后端由root串行编译运行；生产Pg实际数据库连接验证与整体引擎POC尚未执行。
