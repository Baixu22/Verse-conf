# TOML 消融实验：编辑机制 vs 编辑机制 + 校验层（TF-0079）

两臂跑同一批任务，唯一差别是**写入前有没有校验层**。度量刻意不信任门禁的自述：
改动有没有引入高危实例，由 `high_risk_instances` 在改动前后各算一次取差集得出。

## 总览

| 指标 | 只保留编辑机制 | 编辑机制 + 校验层 |
| --- | --- | --- |
| 静默写入高危实例的任务数 | 5 | 0 |
| 拒绝数 | — | 5 |
| 其中确实挡住了一次真实引入 | — | 5 |
| 拒绝信息指出具体实例 | — | 5 |
| 等价安全写法被接受 | — | 5/5 |
| 误拒（79 条预登记中立任务） | 0 | 0 |
| 告警级改动两臂都放行 | 3/3 | 3/3 |

## 逐任务

| 任务 | 分组 | 预期规则 | 只保留编辑机制 | 静默引入 | 编辑机制 + 校验层 | 挡住真实引入 | 等价安全写法 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `disable-tls-verification` | blocking | SEC-005 | 放行 | SEC-005 @ tls_verify | 拒绝（security_rejected） | 是 | 接受 |
| `second-instance-of-an-existing-rule` | blocking | SEC-005 | 放行 | SEC-005 @ tls.verify | 拒绝（security_rejected） | 是 | 接受 |
| `insert-weak-hash-algorithm` | blocking | SEC-001 | 放行 | SEC-001 @ algorithm | 拒绝（security_rejected） | 是 | 接受 |
| `insert-plaintext-password` | blocking | SEC-SENS-001 | 放行 | SEC-SENS-001 @ db_password | 拒绝（security_rejected） | 是 | 接受 |
| `replace-env-reference-with-live-key` | blocking | SEC-SENS-001 | 放行 | SEC-SENS-001 @ api_key | 拒绝（security_rejected） | 是 | 接受 |
| `insert-telnet-port` | advisory | SEC-002 | 放行 | — | 放行 | 否 | — |
| `enable-debug-mode` | advisory | SEC-003 | 放行 | — | 放行 | 否 | — |
| `bind-wildcard-host` | advisory | SEC-004 | 放行 | — | 放行 | 否 | — |

## 这批结果的边界（不要在它之外引用）

- **只测编辑机制与安全门禁的贡献**，不测模型行为：本实验不调用任何模型，两臂的输入完全相同。
- **门禁的契约只覆盖 Critical / High**：`advisory` 组的 3 条改动（SEC-002/003/004）按设计只告警、不阻断，
两臂都放行。这是安全承诺的一条边界，不能把「没有阻断」说成「没有引入风险」。
- **任务集是本轮固定的 8 条**，不是随机抽样：它覆盖全部 6 条规则码，但样本量小，只用于判断
「校验层有没有可测的贡献」，不用于估计真实场景的误拒率。
- **误拒控制用的是 79 条预登记中立任务**：它们刻意门禁中立，所以只能证明门禁不是在一律拒绝，
不能证明它在真实高风险语料上的误拒率。
