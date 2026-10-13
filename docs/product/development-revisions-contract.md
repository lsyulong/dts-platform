# SQL 文档与开发工作流不可变修订合同

修订档案仅支持 `sql_document` 与 `development_workflow`。保存从数据库中的实际当前定义生成档案；浏览器不能提交快照或冒充版本内容。保存不发布定义、不执行 SQL，也不产生成功运行或周期调度记录。恢复替换当前可编辑定义并生成新草稿，发布与执行仍需独立动作。

## 接线

共享入口 `dts_backend::repo::development_revisions`，仓储接口 `DevelopmentRevisionRepo` 提供 `current/save/history/detail/restore` 五个方法，均使用 owned params 或 owned `DevelopmentRevisionScope {tenant_id,actor_id,kind,resource_id}`，历史和详情限定单个实际来源，没有全租户历史接口。

驱动为 `SqliteDevelopmentRevisionRepo::new(pool)`、`PgDevelopmentRevisionRepo::new(pool)`；两库迁移 `023_development_revisions.sql`。共享、驱动导出、工厂、应用、路由与原 `save_version` 桥接由主代理注册，新模块未自行修改这些文件。

服务为 `DevelopmentRevisionsService::new(Arc<dyn DevelopmentRevisionRepo>)`，方法 `save/history/detail/diff/restore`，`handles(kind)` 仅接受两种开发定义。服务复用当前 `product::validation::properties`、`product::references::extract`、`product_jobs::validate_definition`，不引入第二套 SQL 或参数校验。

API 函数在 `api/development_revisions.rs`，函数名同动作。路径为：

- `GET /product/development/{kind}/{id}/revisions?limit=20&offset=0`。
- `GET /product/development/{kind}/{id}/revisions/{revision_id}`。
- `GET /product/development/{kind}/{id}/diff?from_revision_id=...&to_revision_id=...`；省略 `to_revision_id` 表示与实际当前定义对比。
- `POST /product/development/{kind}/{id}/revisions`。
- `POST /product/development/{kind}/{id}/revisions/{revision_id}/restore`。

两类写 DTO 都仅接受 `expected_version/reason/idempotency_key`。DTO 拒绝未知字段、浏览器快照、来源版本覆盖或额外参数。API 读取需要 `development:read`，写需要 `development:write`；仓储在每次事务中重验实际 active 用户、active 租户及真实同租户数据库角色权限。写入同时要求精确 `development:read + development:write`。前端隐藏按钮不能构成授权。

## 快照、哈希与回执

`development_revision` 保存完整真实 `ProductResource` 快照，包括身份、编码、名称、说明、父级、原状态、业务属性、实际版本、创建者、创建/更新时间及真实引用列表。引用按字段、目标种类和 ID 排序，JSON 对象键递归排序，使用紧凑 UTF-8 JSON 计算 SHA256。数组顺序保留，参数、节点和依赖内容没有浏览器重建过程。完整快照限制 320 KiB；业务属性仍受当前 256 KiB 及深度规则约束。

档案包括实际 `source_version/snapshot_hash/saved_by/saved_at/reason`。时间来自数据库时钟。租户/来源种类/来源 ID/实际版本唯一；同版本再次保存返回原档案及 `created=false`，新操作理由保存在其真实回执语义中，不修改第一次保存者、时间或原因。

`ValidatedDevelopmentDefinition` 是服务端专用 witness，内部哈希私有且没有 Deserialize。服务先按当前目录校验属性、只读 SQL AST、类型化参数、实际引用提取及工作流 DAG，再 mint witness；仓储在同一事务里与实际当前快照或不可变修订哈希比较。首次保存缺 witness 拒绝，`None` 仅允许相同原始回执重放。它不是客户端凭据，也不是运行授权。

写入要求稳定 UUID，租户/幂等键唯一。回执比较原 actor、动作、来源身份、原期望版本、实际修订 ID 及完整原因；相同语义返回第一次结果，其他 actor 或语义冲突。当前真实写权限重验后先比较回执，再校验当前状态和版本。来源后来改变时，原成功操作仍可返回冻结回执且 `replayed=true`，不再次覆盖新定义或重复审计。

历史与回执真实 FK 指向来源身份及实际 actor。数据库触发器禁止更新或删除档案、回执，修订历史会阻止来源硬删除，避免断开身份与档案。查询详情核对快照身份、实际版本和 SHA256；完整性异常报错，不将损坏记录当作有效定义。

## 恢复事务

本项目显式撤回使用 `inactive`；恢复仅允许这两类来源的当前 `draft/inactive`，其余状态冲突。当前 `expected_version` 必须匹配，增加实际资源版本并将结果设为 `draft`。旧快照的发布状态、版本、创建者与创建时间不会覆盖当前身份及历史。

恢复读取真实修订，按当前目录重新验证旧 SQL、参数、节点依赖与引用提取。冻结的 `code/name/description/parent_id/properties/references` 恢复为新的可编辑定义，真实目标对象必须仍同租户、同种类且存在。当前引用、外部用户/角色/数据源、父级防环及源删除保护由现有 ProductResource helpers 在同锁事务中重新检查。丢失引用、版本冲突、编码冲突或审计/回执失败会回滚整个恢复。

SQLite 使用 `BEGIN IMMEDIATE`；PostgreSQL 先锁实际租户行，再锁当前 actor、租户及实际角色权限关系，和其他控制面写入使用相同租户串行化方式。定义更新、引用重写、资源审计、冻结回执同一事务提交。审计只保存实际动作、修订 ID、哈希和前后版本，不复制 SQL 文本或参数值。

恢复结果为 `{resource,revision,receipt_id,replayed,requires_publish:true}`。新 `resource.version` 是当前版本加一；历史 `revision.source_version` 仍是原冻结版本。保存结果为 `{revision,receipt_id,created,replayed}`。历史分页 `{data: RevisionSummary[],total}`；详情在 Summary 上追加 `snapshot`，避免列表反复载入完整 SQL 定义。COUNT 与页数据使用同一读取快照，limit 1–100，offset 至多 100 万。

## 对比与执行消费者

对比返回来源身份、两端修订 ID 或当前定义、两端真实版本/哈希、`changes[{path,before,after}]` 与 `truncated`。路径使用 JSON Pointer 转义，比较范围为可编辑定义：编码、名称、说明、父级、业务属性与引用。版本、创建时间及原发布状态单独显示，不把状态差异说成 SQL 内容改变。最多 256 项差异，深度 24，截断明确显示；两份可编辑定义相同可以返回无差异，即使资源版本或完整快照哈希不同。

现有发布消费者使用实际 `ProductResource.version`，实际只读执行 Job 保存独立不可变定义快照及 `source_version`，不接受 revision ID 冒充运行来源。修订 ID 是归档身份。恢复后必须另行发布新实际版本再提交运行；已经提交的真实运行继续保留原有快照和审计。工作流周期、时区、重试配置属于归档属性，本模块没有注册 jiascheduler 周期任务，不将版本保存或手动运行称为周期调度已实现。

## 前端和验证

`DevelopmentRevisionsPage({module})` 封装现有 ProductModulePage，真实分页选择 SQL 文档或工作流；来源使用 `?revision_source=id`，保留其他 query 参数（包括已有 job 深链）。`onActionNavigate` 对 `save_version` 返回 true，校验真实来源身份后选中并滚动面板，自动打开一次带理由确认；期望版本冻结点击行的实际 `record.version`，不以导航后新读取版本静默替换；关闭后不会自动重开。`DevelopmentRevisionPanel({kind,resourceId})` 提供当前定义、修订历史、SQL/节点/参数详情、结构化字段对比及带理由恢复确认。当前页外的历史通过分页获取，不虚构选项。所有 CSS 使用现有绿色主题与已定义 tokens。

typed hooks 位于 `web/src/product/development-revisions`，复用 opaque 会话序号并显式绑定 token。权限切换或身份变化重建状态；旧会话响应不回填。失败保留原期望版本和同语义 UUID，409 不自动覆盖版本重试；关闭 pending 弹窗后的成功回调仅刷新真实查询，不重新打开或展示过期回执。失败读取隐藏旧历史与计数，不显示伪造零条或成功状态。

定向前端 Vitest 10/10 通过（两个测试文件），包含读取不等于写、真实版本和 UUID、失败关闭重试、409 保留原 CAS、已发布禁止恢复/撤回后新草稿、身份切换、真实分页读取失败、关闭 pending、跨租户响应保护、来源深链及来源行版本冻结。定向 Oxlint 无警告或错误，Prettier 已执行。

现有 Python SQLite 实际检查了 001/002/004/023 迁移及重复执行、身份 CHECK、历史/回执不可变、源对象真实 FK；检查通过。新增 2 例共享纯测试、8 例 SQLite 仓储测试和 3 例服务纯测试源码，Rust 执行由主代理统一负责，源码存在不等于测试已通过。rustfmt 解析通过；完整 TS/Vite、Rust 与 HTTP 集成待统一接线验证。本模块没有安装工具、启动服务或进行整体 POC，不代表全部平台范围完成。
