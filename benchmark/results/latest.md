# Agent 编辑保真度基准

- 判定口径版本：`1.2`
- 语料指纹（FNV-1a 64）：`162777399290a695`
- 语料规模：8 篇文档 / 27 个任务
- 复现：`cargo run -p verseconf-bench --release`（语料与判定脚本都在仓库里，不依赖网络或模型）

## 总览

| 策略 | 说明 | 正确率 | 附带损伤率 | 误改率 | 拒绝准确率 | 确定性 | 注释保留 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `verseconf-intent` | 意图契约 + 字符区间最小改动 + 写入前双重校验 | 21/21 (100.0%) | 0/21 (0.0%) | 0/27 (0.0%) | 6/6 (100.0%) | 是 | 21/21 (100.0%) |
| `whole-file-rewrite` | 改值后整个文件按 AI 友好规范形式重新序列化，无写入前校验 | 0/21 (0.0%) | 21/21 (100.0%) | 3/27 (11.1%) | 3/6 (50.0%) | 是 | 19/21 (90.5%) |
| `line-diff` | 按字段名找第一处匹配行并整行替换，无路径解析、无写入前校验 | 6/21 (28.6%) | 6/21 (28.6%) | 10/27 (37.0%) | 2/6 (33.3%) | 是 | 8/21 (38.1%) |

口径：正确 = 目标值改对且改动区间之外字节零变化；附带损伤 = 值改对了但区间之外也变了；误改 = 值不对 / 目标不对 / 结果无法解析 / 该拒未拒；误拒 = 该改却拒绝；拒绝准确率要求错误码与期望一致。

## 逐任务结果

### `verseconf-intent`

| 任务 | 期望 | 结果 | 说明 |
| --- | --- | --- | --- |
| `set-nested-with-comment` | applied | 正确 | 目标值已改对，改动区间之外字节零变化 |
| `set-deep-nested` | applied | 正确 | 目标值已改对，改动区间之外字节零变化 |
| `set-unusual-spacing` | applied | 正确 | 目标值已改对，改动区间之外字节零变化 |
| `set-with-expect` | applied | 正确 | 目标值已改对，改动区间之外字节零变化 |
| `set-crlf` | applied | 正确 | 目标值已改对，改动区间之外字节零变化 |
| `set-root-scalar` | applied | 正确 | 目标值已改对，改动区间之外字节零变化 |
| `set-quoted-value` | applied | 正确 | 目标值已改对，改动区间之外字节零变化 |
| `set-named-list-element` | applied | 正确 | 目标值已改对，改动区间之外字节零变化 |
| `refuse-ambiguous` | refused(target_ambiguous) | 正确拒绝 | 按 target_ambiguous 拒绝：目标有歧义：servers[name="primary"].ip 命中了 2 个元素，必须唯一命中 |
| `refuse-missing` | refused(target_not_found) | 正确拒绝 | 按 target_not_found 拒绝：目标不存在：server.missing |
| `refuse-stale-expect` | refused(expectation_mismatch) | 正确拒绝 | 按 expectation_mismatch 拒绝：前置条件不符：server.port 期望 1234，实际 8080 |
| `refuse-schema-break` | refused(validation_failed) | 正确拒绝 | 按 validation_failed 拒绝：改动后校验失败：<result>（Validation error at 5:5: type mismatch for field 'port': expected integer, found "not-a-number" (type: string)） |
| `refuse-security` | refused(security_rejected) | 正确拒绝 | 按 security_rejected 拒绝：改动引入新的安全风险：<result>（SEC-005 @ tls.ssl_verify） |
| `refuse-invalid-plan` | refused(invalid_plan) | 正确拒绝 | 按 invalid_plan 拒绝：unsupported_version: 契约版本 '9.9' 不受支持，当前只接受 1.0 |
| `insert-root-key` | applied | 正确 | 目标已插入，插入区间之外字节零变化 |
| `insert-nested-key` | applied | 正确 | 目标已插入，插入区间之外字节零变化 |
| `insert-into-inline-table-scope` | applied | 正确 | 目标已插入，插入区间之外字节零变化 |
| `delete-root-key` | applied | 正确 | 目标已删除，删除区间之外字节零变化 |
| `delete-nested-key-with-metadata` | applied | 正确 | 目标已删除，删除区间之外字节零变化 |
| `delete-named-list-field` | applied | 正确 | 目标已删除，删除区间之外字节零变化 |
| `multi-set-two-tables` | applied | 正确 | 2 个目标全部成立，改动之外的行与字节零变化 |
| `multi-set-and-delete` | applied | 正确 | 2 个目标全部成立，改动之外的行与字节零变化 |
| `multi-set-same-named-element` | applied | 正确 | 2 个目标全部成立，改动之外的行与字节零变化 |
| `cross-set-included-scalar` | applied | 正确 | 目标值已改对，改动区间之外字节零变化 |
| `cross-set-included-with-comment` | applied | 正确 | 目标值已改对，改动区间之外字节零变化 |
| `cross-set-root-key` | applied | 正确 | 目标值已改对，改动区间之外字节零变化 |
| `cross-delete-included-key` | applied | 正确 | 目标已删除，删除区间之外字节零变化 |

### `whole-file-rewrite`

| 任务 | 期望 | 结果 | 说明 |
| --- | --- | --- | --- |
| `set-nested-with-comment` | applied | 附带损伤 | 改动区间之前与之后的字节也变了：行数 23→21，不同行 17，CRLF 否→否 |
| `set-deep-nested` | applied | 附带损伤 | 改动区间之前与之后的字节也变了：行数 23→21，不同行 17，CRLF 否→否 |
| `set-unusual-spacing` | applied | 附带损伤 | 改动区间之前与之后的字节也变了：行数 23→21，不同行 17，CRLF 否→否 |
| `set-with-expect` | applied | 附带损伤 | 改动区间之前与之后的字节也变了：行数 23→21，不同行 17，CRLF 否→否 |
| `set-crlf` | applied | 附带损伤 | 改动区间之前与之后的字节也变了：行数 5→5，不同行 1，CRLF 是→否 |
| `set-root-scalar` | applied | 附带损伤 | 改动区间之前与之后的字节也变了：行数 6→9，不同行 8，CRLF 否→否 |
| `set-quoted-value` | applied | 附带损伤 | 改动区间之前的字节也变了：行数 6→9，不同行 8，CRLF 否→否 |
| `set-named-list-element` | applied | 附带损伤 | 改动区间之前的字节也变了：行数 16→14，不同行 13，CRLF 否→否 |
| `refuse-ambiguous` | refused(target_ambiguous) | 正确拒绝 | 按 target_ambiguous 拒绝：目标有歧义：servers[name="primary"].ip 命中了 2 个元素，必须唯一命中 |
| `refuse-missing` | refused(target_not_found) | 正确拒绝 | 按 target_not_found 拒绝：目标不存在：server.missing |
| `refuse-stale-expect` | refused(expectation_mismatch) | 该拒未拒 | 应以 expectation_mismatch 拒绝，实际改动了文件 |
| `refuse-schema-break` | refused(validation_failed) | 该拒未拒 | 应以 validation_failed 拒绝，实际改动了文件 |
| `refuse-security` | refused(security_rejected) | 该拒未拒 | 应以 security_rejected 拒绝，实际改动了文件 |
| `refuse-invalid-plan` | refused(invalid_plan) | 正确拒绝 | 按 invalid_plan 拒绝：unsupported_version: 契约版本 '9.9' 不受支持，当前只接受 1.0 |
| `insert-root-key` | applied | 附带损伤 | 改动之外的字节也变了：行数 6→10，不同行 9，CRLF 否→否 |
| `insert-nested-key` | applied | 附带损伤 | 改动之外的字节也变了：行数 23→22，不同行 17，CRLF 否→否 |
| `insert-into-inline-table-scope` | applied | 附带损伤 | 改动之外的字节也变了：行数 6→10，不同行 9，CRLF 否→否 |
| `delete-root-key` | applied | 附带损伤 | 改动之外的字节也变了：行数 6→8，不同行 7，CRLF 否→否 |
| `delete-nested-key-with-metadata` | applied | 附带损伤 | 改动之外的字节也变了：行数 23→20，不同行 17，CRLF 否→否 |
| `delete-named-list-field` | applied | 附带损伤 | 改动之外的字节也变了：行数 16→13，不同行 13，CRLF 否→否 |
| `multi-set-two-tables` | applied | 附带损伤 | 目标之外的前缀字节被改动：应原样保留 "# 服务端配置\\n# 这份文档专门用来检验「改动之外字节零变化」：\\n# 有多余…" |
| `multi-set-and-delete` | applied | 附带损伤 | 目标之外的前缀字节被改动：应原样保留 "# 内联表、数组与非常规空白：值替换不该动到别的写法\\napp = { name…" |
| `multi-set-same-named-element` | applied | 附带损伤 | 目标之外的前缀字节被改动：应原样保留 "# 命名列表：按 name 字段定位唯一元素。\\n# 前两条同名，任何\"挑第一个…" |
| `cross-set-included-scalar` | applied | 附带损伤 | 改动区间之前与之后的字节也变了：行数 9→8，不同行 8，CRLF 否→否 |
| `cross-set-included-with-comment` | applied | 附带损伤 | 改动区间之前与之后的字节也变了：行数 9→8，不同行 8，CRLF 否→否 |
| `cross-set-root-key` | applied | 附带损伤 | 改动区间之前与之后的字节也变了：行数 6→5，不同行 5，CRLF 否→否 |
| `cross-delete-included-key` | applied | 附带损伤 | 改动之外的字节也变了：行数 9→7，不同行 8，CRLF 否→否 |

### `line-diff`

| 任务 | 期望 | 结果 | 说明 |
| --- | --- | --- | --- |
| `set-nested-with-comment` | applied | 附带损伤 | 改动区间之后的字节也变了：行数 23→23，不同行 1，CRLF 否→否 |
| `set-deep-nested` | applied | 误改 | 目标值应为 5433，实际为 5432 |
| `set-unusual-spacing` | applied | 误改 | 目标值应为 "db2.internal"，实际为 "db.internal" |
| `set-with-expect` | applied | 正确 | 目标值已改对，改动区间之外字节零变化 |
| `set-crlf` | applied | 附带损伤 | 改动区间之后的字节也变了：行数 5→5，不同行 1，CRLF 是→是 |
| `set-root-scalar` | applied | 附带损伤 | 改动区间之前的字节也变了：行数 6→6，不同行 1，CRLF 否→否 |
| `set-quoted-value` | applied | 正确 | 目标值已改对，改动区间之外字节零变化 |
| `set-named-list-element` | applied | 误改 | 目标值应为 20，实际为 10 |
| `refuse-ambiguous` | refused(target_ambiguous) | 该拒未拒 | 应以 target_ambiguous 拒绝，实际改动了文件 |
| `refuse-missing` | refused(target_not_found) | 正确拒绝 | 按 target_not_found 拒绝：找不到字段 'missing' 所在的行 |
| `refuse-stale-expect` | refused(expectation_mismatch) | 该拒未拒 | 应以 expectation_mismatch 拒绝，实际改动了文件 |
| `refuse-schema-break` | refused(validation_failed) | 该拒未拒 | 应以 validation_failed 拒绝，实际改动了文件 |
| `refuse-security` | refused(security_rejected) | 该拒未拒 | 应以 security_rejected 拒绝，实际改动了文件 |
| `refuse-invalid-plan` | refused(invalid_plan) | 正确拒绝 | 按 invalid_plan 拒绝：unsupported_version: 契约版本 '9.9' 不受支持，当前只接受 1.0 |
| `insert-root-key` | applied | 正确 | 目标已插入，插入区间之外字节零变化 |
| `insert-nested-key` | applied | 误改 | 目标值应为 4，实际为 <缺失> |
| `insert-into-inline-table-scope` | applied | 正确 | 目标已插入，插入区间之外字节零变化 |
| `delete-root-key` | applied | 正确 | 目标已删除，删除区间之外字节零变化 |
| `delete-nested-key-with-metadata` | applied | 正确 | 目标已删除，删除区间之外字节零变化 |
| `delete-named-list-field` | applied | 误改 | 目标 servers[name="replica"].weight 应被删除，实际仍然存在：10 |
| `multi-set-two-tables` | applied | 附带损伤 | 目标之外的后缀字节被改动：应原样保留 " #@ type_hint=\"string\"\\n  output = [\"std…" |
| `multi-set-and-delete` | applied | 附带损伤 | 目标之外的前缀字节被改动：应原样保留 "# 内联表、数组与非常规空白：值替换不该动到别的写法\\napp = { name…" |
| `multi-set-same-named-element` | applied | 误改 | 目标 servers[name="replica"].ip 应为 "10.0.0.30"，实际为 "10.0.0.3" |
| `cross-set-included-scalar` | applied | 误拒 | 应当改动，实际以 target_not_found 拒绝 |
| `cross-set-included-with-comment` | applied | 误拒 | 应当改动，实际以 target_not_found 拒绝 |
| `cross-set-root-key` | applied | 附带损伤 | 改动区间之后的字节也变了：行数 6→6，不同行 1，CRLF 否→否 |
| `cross-delete-included-key` | applied | 误拒 | 应当改动，实际以 target_not_found 拒绝 |
