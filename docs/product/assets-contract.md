# 资产目录、关系地图与已记录使用统计

资产目录使用真实 `product_resource(asset_entry)`，没有复制假的元数据、用户或模型。`object_kind` 固定为 metadata_object、data_model、atomic_metric、derived_metric、composite_metric、field_standard、enum_standard、unit_standard；`object_id` 是同租户真实资源引用。负责人 `owner_id` 必须是同租户有效用户，数据域引用真实 warehouse_domain。注册、发布、撤回、引用替换和审计在同一仓储事务完成；SQLite 提前保留写锁，PostgreSQL 锁定实际租户、用户和权限行。

状态是 draft → registered → published；registered/published 可撤回为 withdrawn，withdrawn 可编辑成新 draft 或再次登记。已登记或发布版本必须先撤回再编辑。登记验证实际来源，发布保存不可变资产版本、来源版本、来源属性快照、定义哈希、证明、实际操作人和时间。已有发布历史禁止硬删，以保留真实审核轨迹与来源外键。

发布范围为 `catalog_definition`，返回 `physical_published:false`。它表示目录版本已经冻结；没有执行数据查询、物理表创建、指标计算或分发。元数据来源必须有当前采集登记、版本、全字段指纹与受控引擎绑定证明。已声明物理发布的数据模型必须有真正模型发布器写入的匹配版本回执，不能仅以 status=published 作为证明。模型草稿、指标和标准来源可登记为实际保存的定义，证明明确为 stored_definition，`physical_data_verified:false`。服务先验证实际定义，再由仓储在写锁内核对相同来源版本及定义哈希，防止验证后换源。

`AssetView` 返回 resource、publication、source_valid、stale、invalid_reason、current_source_version。每次读取根据当前真实来源重新判断；采集失效、源版本/哈希变化、缺少回执或回执版本不符均不会显示为当前有效发布。采集证明代表最近已提交的采集结果，目录查看不会声称重新探测了外部物理表。

登记要求实际 assets:write 与来源 read 权限；发布和撤回要求 assets:execute 与来源 read；目录、地图和使用分析要求 assets:read 与相关来源 read。权限从当前有效用户、租户和同租户角色的明确权限读取。目录 SQL 计数和分页同时限制可读来源类型；不在分页后才过滤。地图移除不可读来源节点，不返回它们的对象 ID 或名称。

资产地图保存真实 asset_entry 节点引用，最多 500 节点、2000 条边。边来自真实来源资源的已保存引用，或经 `MetadataLineageRepo` 再次验证 metadata:read 的当前人工血缘投影。边携带来源版本、字段，或实际血缘 ID、版本、操作人、时间和人工证据。没有真实关系不会造边，没有真实资产不会造节点。来源或血缘版本不符列为 stale；非法节点列为 invalid；节点、边和血缘扫描上限都返回 truncated 与原因。资产与血缘快照分别读取，投影严格核对两端版本；不承诺远程引擎和控制数据库之间的同时刻原子快照。

使用分析仅对真实元数据资产接入 `data_access_audit` 的 query 事件，按同租户、实际元数据 ID 与 `[period_start,period_end)` 窗口查询。结果范围是已记录的受控查询事件，不能解读为企业所有访问。最多 366 天且不得请求未来窗口，计数上限 5000；超过上限返回 complete=false、真实匹配事件数量，各完整窗口统计值为空，不输出部分计数冒充全窗口。没有埋点的模型、指标、标准返回 not_instrumented 与空统计值，不能解读为全部访问为零。元数据的零记录只表示这段时间该审计来源没有匹配事件。

本模块不含 API 调用、应用凭据、下游数据分发或资产审批。测试源包括 8 个 SQLite 事务/权限/历史/窗口边界测试与 4 个地图投影测试；完整平台 POC 按用户要求放到后续集中验证。
