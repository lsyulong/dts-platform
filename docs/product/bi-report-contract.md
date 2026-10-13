# 真实 BI 报表执行合同

本文是本产品的实施合同，不是对购买原型内部结构的推测。当前实现覆盖报表定义、实际元数据校验、定义发布、持久化只读聚合运行、真实结果读取、取消和重试。共享链接、外发、导出文件、周期运行和任意 SQL 数据集需要各自专属业务闭环，不能仅保存配置就宣称可用。

## 定义与真实来源

`bi_report.properties` 的 `metadata_object_id` 必须指向同租户已实际采集的表或视图。后台验证内部 `MetadataCatalogRepo` 的来源证明，包括 SwanLake 引擎绑定、租户、对象版本、catalog/schema/table、实际字段和结构指纹。用户复制 `engine_binding` 属性或新建同名元数据不能获得内部证明。

`model_id` 仅作为可选逻辑模型关联，不能代替实际查询来源。非空自由 `sql` 明确拒绝，不会悄悄忽略。模型写入和报表只读分析分别执行，报表发布不触发物理模型发布。

```json
{
  "metadata_object_id": "实际采集对象的ID",
  "dimensions": ["region"],
  "measures": [
    {"name": "total_amount", "expression": "amount", "aggregate": "sum"},
    {"name": "total_orders", "expression": "*", "aggregate": "count"}
  ],
  "row_limit": 500,
  "chart_config": {
    "type": "bar",
    "title": "区域订单金额",
    "x_field": "region",
    "y_field": "total_amount",
    "legend": true
  }
}
```

维度为实际字段名称数组，允许 0 至 16 个。度量为 1 至 32 项，聚合枚举为 `sum/avg/count/min/max`；表达式只允许实际单字段，`count` 还允许 `*`。SUM/AVG 必须采用真实数值类型。结果别名必须唯一，不能与任何源字段同名，避免字段授权的别名遮蔽问题。结果行上限为 1 至 1000，默认 500。

表格支持多维或无维聚合；当前柱图、折线图、饼图要求一个实际维度和一个数值度量，图表字段必须来自实际输出维度/度量。文本或日期 MIN/MAX 可在表格显示。图表配置只决定展示方式，不产生模拟指标。标识符使用 AST 引用，支持中文和空格；V1 明确拒绝带双引号、反斜线、控制字符、空值或超过 256 字节的名称。

## 校验、发布与运行

`BiReportService::validate_saved` 返回已校验的维度、度量、实际元数据身份/版本与行上限，并明确 `executed=false`、`schedule_registered=false`。校验不调用分析引擎。发布需要实际 `bi:read`、`bi:write`、`bi:execute` 权限，乐观锁保护定义版本，状态可由 `draft/inactive/publish_failed` 改为 `published`；响应 `publication_scope="validated_bi_definition"`。

提交运行需要当前活动身份、`bi:read`、`bi:execute`、已发布定义、相符版本与同租户引擎绑定。输入为 `{expected_version,idempotency_key}`；后台冻结报表属性、已验证的实际元数据及类型化报表定义，创建 `source_kind="bi_report"`、`action="execute_bi"` 的真实 ProductJob。初始状态为 queued；发布或提交不伪造成功数据。

运行快照 `format_version=1`、`executor="bi_aggregate_v1"`，包含实际来源及版本、完整冻结元数据及内部证明、类型化维度/度量/图表定义。执行器重新验证快照和唯一 `report` 节点的定义深相等；来源被撤回、版本改变或内部证明失效时停止。执行控制复用真实租约、fence、续期、取消和只读幂等重试，合同见 [read-only-job-contract.md](read-only-job-contract.md)。

SQL 由固定模板构建 AST，将真实三段表身份及字段插入 AST 引用；聚合函数只从枚举映射，未拼接用户表达式。查询只读重验后，通过实际租户绑定的 QueryService 执行。分组维度提供稳定排序，无维聚合不添加分组排序。查询使用 `LIMIT(row_limit + 1)` 多读取一个检测行；实际结果超过 row_limit 时节点失败，不保存截断的“完整”结果。

## 数据授权与结果

每次执行需要统一 QueryAuthorizationService，执行前、执行期间、返回后重验实际用户的数据授权并记录审计。单表聚合可使用实际字段授权；投影、条件、分组和排序引用的字段均纳入授权，COUNT(*) 需要该表完整字段授权。当前不接受任意复杂表达式作为度量。

节点结果只能由真实 QueryResult 生成。字段名称和顺序须与报表定义一致，行数须等于实际 rows 长度，每行字段集合须完整匹配；图表 Y 返回类型必须实际数值。返回结构包含 `metadata_id/version`、`dimensions`、`measures`、`chart_config`、实际 `columns/rows/row_count`、`row_limit` 与 `complete=true`。空结果仍保留实际空数组，不填充演示数据。任何结构、行限额或存储字节预算溢出都使节点失败；查询结果保存遵守 ProductJob 256 KiB 总预算。

运行列表不返回数据行。历史详情、幂等重复提交及取消响应使用 reader-specific ProductJobView；重新校验当前查看者的实际字段授权和冻结实际元数据内部证明，即使查看者有 all_data 也不跳过报表来源证明。授权失效、服务未配置或审计无法保存时结果为空且 `result_access="restricted"`，真实 succeeded 状态及数据库结果不改变。快照、运行参数和内部节点定义不经运行响应泄露。恢复合法授权后可重新读取。

实际查询数据还经过统一 MaskingService，结果保存后有覆盖完整 QueryResult 的 HMAC 收据；历史读取按当前用户策略再次检查，同策略不重复摘要，策略变化无法恢复原值时明确受限。适用脱敏字段用于聚合、分组、排序或派生表达式的情况 V1 明确拒绝，防止从计算结果旁路推断被保护值；不会把 HMAC 文本当真实数值聚合。仅适用当前用户的已发布用户组与脱敏规则参与策略计算，可信规则绑定实际元数据证明。

## 本轮验证边界

单元测试源覆盖真实三段表及字段 AST 编译、超量检测行、任意表达式/自由 SQL/遮蔽别名拒绝、伪造快照/节点拒绝、实际返回类型/字段校验、图表维度约束。SQLite + 实际 Sa 会话服务测试先走实际 MetadataService→ProductJob worker→MetadataCatalogRepo 原子反射，再校验报表发布不调用引擎、真实聚合运行持久化、超量节点失败、虚假来源证明拒绝、历史授权撤回后隐藏结果且保留真实数据库记录。

这些测试使用明确的测试查询观测接口替代外部引擎网络调用，没有启动平台或 SwanLake；编译与定向测试由主代理使用现有工具串行验证，整体 POC 留在平台业务模块完成之后。
