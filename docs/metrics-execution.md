# 指标定义、发布和受控计算

本模块覆盖原子指标、派生指标、复合指标、修饰词和时间周期。`services/metrics` 负责类型校验、当前物理模型与元数据证明、版本冻结、编译和真实结果映射。任务执行沿用 ProductJob、查询字段授权、脱敏、租约和历史结果授权，指标服务不会自行访问引擎或生成成功样本。前后端注册与任务执行器接线由主任务统一完成，本文记录源码合同，不表示已经进行端到端 POC。

## 定义字段

| 实体 | 可编辑字段 | 真实约束 |
| --- | --- | --- |
| `atomic_metric` | `process_id`、`model_id`、`metadata_object_id`、`expression`、`grain`、`grain_columns`、`time_column`、`time_timezone`、`dimensions`、可选 `unit_id` | 业务过程粒度文本一致；模型须已物理发布；实际基础表、字段类型、非空主键及 catalog/schema/table 与发布回执一致；粒度字段按顺序对应模型主键 |
| `derived_metric` | `atomic_metric_id`、可选 `modifier_id`、`period_id`、`dimensions` | 依赖均须有当前服务端发布证明；修饰词绑定同版本元数据；周期和原子时间字段业务时区一致 |
| `composite_metric` | `metric_ids`、`expression`、可选 `unit_id` | 依赖编码用于公式；每个选定依赖必须实际引用；同表、同模型版本、相同主键粒度、输出维度和时间字段；禁止循环，最多16层、64个冻结对象、32个度量 |
| `metric_modifier` | `metadata_object_id`、`dimension`、`condition` | 实际维度字段与常量进行类型匹配的比较、IN、空值判断和布尔连接；拒绝子查询、任意函数、脚本和尾随语句 |
| `metric_period` | `unit`、`length`、`timezone` | `day/week/month/year`；长度1至10000；有效 IANA 时区；执行窗口最多3660天 |

`publication` 是服务端只读冻结属性，公共 create/update 必须剥离客户端此字段。修改已发布定义应保存为草稿，删除旧发布依赖链接并重新发布。发布追加 `publication_dependency` 真实资源链接，保持引用删除约束；它不表示物理执行成功。模型回执须包含真正发布或物理核验产生的 `physical_binding` 与 `physical_columns`。历史回执缺少 catalog 定位时拒绝指标发布，不能仅凭 `status=published` 放行。

单个原子度量为 SUM/AVG/COUNT/MIN/MAX；除 COUNT 的字段或星号外，参数只允许实际数值列和有限十进制加减乘除。首轮不支持 DISTINCT、窗口函数、自由 SQL、跨表指标或自定义执行器。维度允许真实可分组标量，禁止 JSON/BLOB 以及覆盖 `metric_value/input_rows` 输出名。

## 状态和动作

定义从 `draft` 发布到 `published`，撤回后为 `inactive`。发布冻结当前模型、物理回执、真实元数据、单位、修饰词、周期与子指标版本；修改或撤回任何依赖后，执行及历史结果释放都会重新读取实际资源和内部采集证明并拒绝过时定义。发布只确认真实绑定和冻结定义；只有 ProductJob 成功执行只读 SQL 后才产生计算结果。

公开服务合同：

```rust
MetricsService::new(resources, jobs, metadata, auth, query_access)
    .with_query_authorization(query_authorization);
validate_saved(actor, kind, id, expected_version) -> Value
publish(actor, kind, id, expected_version) -> Value
withdraw(actor, kind, id, expected_version) -> Value
enqueue(actor, kind, id, MetricRunRequest) -> EnqueuedProductJob
```

权限采用实际 Sa-token 数据库权限：读取 `metrics:read`，发布/撤回 `metrics:write`，提交计算额外 `metrics:execute`。执行器和历史视图使用 `authorize_metric` / `authorize_metric_result`：服务端重读真实 job/node，检查当前租约或终态、精确重新编译 SQL、所有依赖和物理元数据，再执行普通 SELECT 字段授权。这个作用域不赋予任意 SQL 通行，也不具有质量安全计数的脱敏例外。被脱敏策略禁止的聚合或条件仍会拒绝。

## 数据窗口与幂等

请求字段为 `expected_version`、稳定 `idempotency_key`、`row_limit`（1至1000）以及数据窗口：

- 原子或无周期复合计算：明确 UTC RFC3339 `window_start/window_end`，半开区间 `[start,end)`，不接受被忽略的 `as_of`。
- 含周期计算：稳定 UTC `as_of`，不接受手动起止覆盖。按周期时区取当前自然期开始作为终点，向前计算完整结束的 N 期；周从周一开始，月从1日开始，年从1月1日开始。
- 夏令时采用真实 IANA 规则。不存在或歧义的本地午夜明确拒绝；无时区 TIMESTAMP 的窗口端点也不得处于歧义本地时间。DATE 窗口须对齐业务时区午夜。
- 复合依赖在实际运行时必须得到相同窗口。多个不同周期如果生成不同窗口直接拒绝，不静默拼接不同时间口径。

快照 format=1、executor=`metric_aggregate_v1`，action=`execute_metric`；一个节点 id=`metric`、type=`metric_aggregation`。节点定义与固定快照必须一致；`can_retry=true` 仅表示已验证只读查询。SQL 使用准确三段物理定位、安全引号、真实字段、受控常量、固定分组与排序及 `row_limit+1` 溢出哨兵；超限拒绝生成完整结果，不能悄悄截断。

任务执行器合同：

```rust
prepare_claimed(&ProductJob) -> Vec<(node_id, sql)>
verify_current(&dyn ProductResourceRepo, &dyn MetadataCatalogRepo, &ProductJob) -> ()
map_node_result(&ProductJob, QueryResult) -> Value
```

## 数值与真实结果

输出保留 `columns/rows/row_count`、真实 `metric_value/input_rows`、元数据和模型版本、冻结依赖版本、数据窗口、单位及 `result_states`。空输入为 `empty_data`，存在输入但聚合值为 NULL 为 `undefined_value`，有实际值为 `calculated`；除零使用 NULLIF，返回 NULL，不造零值或假成功。`input_rows` 表示该分组在共同数据窗内的实际输入行数，修饰条件在每个度量内部应用，因此它不是每个修饰条件的命中行数。

Decimal 使用实际 Arrow precision/scale 校验的精确文本，大整数保留精确整数文本，禁止转 f64 再输出。SUM/MIN/MAX、整数/Decimal 加减乘法采用引擎原生精确口径；实际返回浮点却声称精确时拒绝。AVG、显式除法、原始 FLOAT/DOUBLE 度量为 `engine_native_approximate`。单位先转换到共同基准再输出到目标单位；转换系数及输出倒数必须能用有限18位十进制表示，不能精确转换时拒绝。首轮单位维度为单个标准维度或 `dimensionless`，复合维度不匹配时拒绝，不能凭单位名称猜测汇率或跨维度换算。

COUNT(*) 要求完整表字段授权；即使修饰条件把它编译成 COUNT(CASE ...) 也再次检查完整字段授权。其他度量检查真正引用的值、分组、条件、时间和主键计数字段。撤回会阻止后续授权及结果释放，不能保证物理取消已经发给远端引擎的在途查询。

当前模型物理回执核验使用租户内有界查询，最多1000条已发布回执；超过容量明确拒绝，应后续增加专属发布索引，不能默认选择错误回执。冻结证明最多192KiB，查询最多64KiB，真实结果沿现有256KiB任务预算。暂无实际结果、引擎错误、授权拒绝和数据超限均保留为真实状态。

## 前端落地建议

原子指标表单应让用户选择真实采集表后按实际字段选择主键粒度、时间字段和分组维度；业务粒度文本可从选定业务过程带入。修饰词同样先选实际表、再选实际维度，提供类型对应的常量条件输入。复合公式展示选定指标实际编码及单位，避免让用户把资源ID当SQL字段输入。计算对话框根据冻结周期显示起止窗口或 as_of，两者不能同时提交。结果页面显示真实窗口、版本和空/未定义状态，Decimal/大整数文本直接展示。原子、派生、复合的任务历史必须使用统一受控 job 详情接口；不把公共资源配置页当作计算结果表。
