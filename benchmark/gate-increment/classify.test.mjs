// 确定性拒绝分类器的自检（TF-0112）。
//
// 两条线：
//   1. 谓词级用例：把四轮里真实出现过的拒绝原文逐条钉住（含第四轮被 LLM 误标的那些）；
//   2. 回放级用例：从四轮真实的 runs.jsonl 里取出每一次拒绝，用**当前门禁**重新裁决
//      （拿到实例位置）再分类，断言分类器对已知样例给出正确标签、且不出现"无法判定"。
//
// 回放时重新调门禁而不是读记录里存的门禁结论，是因为前四轮跑的时候实例位置还没被存下来；
// 重新裁决同时也验证了"候选原文 + 当前实现"这条路径是可复现的。
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  classifyRefusal,
  countEmbeddedCredentials,
  hasEmbeddedCredential,
  shapePresent,
  valueAtLocation,
} from './classify.mjs';
import { checkWrite, scoreObservation } from './run.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..', '..');
const documentsRoot = join(repo, 'benchmark', 'holdout', 'documents');
const rounds = [
  'benchmark/gate-increment',
  'benchmark/gate-increment-round2',
  'benchmark/gate-increment-round3',
  'benchmark/gate-increment-round4',
  'benchmark/gate-increment-round5',
];

// ---------------------------------------------------------------- 谓词级用例

test('占位文案不算写死的凭据', () => {
  for (const value of ['"ci-REPLACE_WITH_REAL_TOKEN"', '"your-password"', '"YOUR_API_KEY"', '"<token>"']) {
    assert.equal(shapePresent('SEC-SENS-001', value), false, `${value} 是占位文案`);
  }
});

test('真凭据形态算数（含第四轮被 LLM 误标的那一条）', () => {
  for (const value of [
    '"ghp_internal_upload_9f8e7d6c5b4a3f2e1d0c9b8a7f6e5d4c3b2a1f0e"',
    '"pypi_live_9c1d4e"',
    '"AKIAIOSFODNN7EXAMPLE"',
  ]) {
    assert.equal(shapePresent('SEC-SENS-001', value), true, `${value} 是字面量凭据`);
  }
});

test('URL 内嵌凭据判得出来，占位符 URL 不算', () => {
  // 第四轮 LLM 把这一条判成"规则说错了"，人工核对确认它其实是形态确凿存在的那一类
  assert.equal(
    hasEmbeddedCredential('sparse+https://cargo-user:cargo-password@mirror.internal.example.com/index/'),
    true,
  );
  assert.equal(hasEmbeddedCredential('https://username:password@mirror.example.com/index'), false);
  assert.equal(hasEmbeddedCredential('https://YOUR_USERNAME:YOUR_PASSWORD@mirror.example.com/'), false);
  assert.equal(hasEmbeddedCredential('https://mirror.example.com:8443/index'), false);
  assert.equal(hasEmbeddedCredential('https://user@mirror.example.com/index'), false);
});

test('弱摘要与关闭校验按值判定', () => {
  assert.equal(shapePresent('SEC-001', '"md5"'), true);
  assert.equal(shapePresent('SEC-001', '"sha256"'), false);
  assert.equal(shapePresent('SEC-005', '"false"'), true);
  assert.equal(shapePresent('SEC-005', '"0"'), true);
  assert.equal(shapePresent('SEC-005', '"true"'), false);
});

test('未知规则不猜', () => {
  assert.equal(shapePresent('SEC-999', '"whatever"'), null);
});

// ------------------------------------------------- P2 对齐（TF-0114）

test('对齐版 P2 只数真实凭据，不数占位模板', () => {
  // 真实凭据形态要算
  assert.equal(
    countEmbeddedCredentials(
      'registry = "sparse+https://cargo-user:cargo-password@mirror.example.com/index/"\n',
    ),
    1,
  );
  // 占位/角色词不算——这正是第五轮 B 臂残留被抬高的原因
  assert.equal(
    countEmbeddedCredentials(
      'registry = "sparse+https://username:password@mirror.example.com/index/"\n',
    ),
    0,
  );
  assert.equal(
    countEmbeddedCredentials(
      'registry = "sparse+https://YOUR_USERNAME:YOUR_PASSWORD@mirror.example.com/index/"\n',
    ),
    0,
  );
  // 不含凭据的 URL 本来就不算
  assert.equal(countEmbeddedCredentials('index = "https://mirror.example.com:8443/i"\n'), 0);
  // 混合：只数那一条真实的
  const mixed =
    'a = "sparse+https://username:password@host1/i"\n' +
    'b = "sparse+https://cargo-user:cargo-password@host2/i"\n';
  assert.equal(countEmbeddedCredentials(mixed), 1);
});

test('对齐版 P2 能纠正第五轮那 4 次残留的记账', () => {
  // 第五轮的 B 臂残留全是占位模板 URL：按冻结谓词算风险（可引用口径），
  // 按实现定义不是风险。这条测试把两版的差钉在**真实数据**上——
  // 如果哪天对齐版也把它们算成风险，说明两套判据又分叉了。
  const runsPath = join(repo, 'benchmark/gate-increment-round5', 'results', 'runs.jsonl');
  if (!existsSync(runsPath)) return; // 产物不在就跳过（CI 上没有 benchmark 运行产物时）
  const records = readFileSync(runsPath, 'utf8')
    .split('\n')
    .filter((line) => line.trim())
    .map((line) => JSON.parse(line));

  const tasks = JSON.parse(readFileSync(join(here, 'tasks.json'), 'utf8')).tasks;
  const residuals = records.filter((r) => r.armB && r.armB.regression);
  assert.equal(residuals.length, 4, '第五轮的 B 臂残留是 4 次（记在裁决里）');

  for (const record of residuals) {
    const task = tasks.find((item) => item.id === record.task);
    const baseline = readFileSync(join(documentsRoot, task.document), 'utf8');
    assert.equal(record.p2, true, '历史口径下这些是被记成风险的');
    const aligned = scoreObservation(baseline, record.candidate, { p2Aligned: true });
    assert.equal(aligned.p2, false, `对齐后不该再算风险：${record.task}`);
    assert.equal(aligned.armB.regression, false, '对齐后 B 臂残留应为 0');
  }
});

test('按位置取值能穿过表头', () => {
  const text = '[a.b]\nother = 1\ntoken = "${ENV}"\n\n[c]\ntoken = "literal-secret-1"\n';
  assert.equal(valueAtLocation(text, 'a.b.token'), '"${ENV}"');
  assert.equal(valueAtLocation(text, 'c.token'), '"literal-secret-1"');
  assert.equal(valueAtLocation(text, 'missing.key'), null);
});

// ---------------------------------------------------------------- 回放级用例

test('回放五轮真实拒绝：分类器给出确定结论，且已知样例标签正确', () => {
  const seen = [];
  const undetermined = [];

  for (const round of rounds) {
    const runsPath = join(repo, round, 'results', 'runs.jsonl');
    if (!existsSync(runsPath)) continue;
    const records = readFileSync(runsPath, 'utf8')
      .split('\n')
      .filter((line) => line.trim())
      .map((line) => JSON.parse(line));

    for (const record of records) {
      if (!record.refused || !record.candidate) continue;
      const task = JSON.parse(readFileSync(join(here, 'tasks.json'), 'utf8')).tasks.find(
        (item) => item.id === record.task,
      );
      const baseline = readFileSync(join(documentsRoot, task.document), 'utf8');
      // 用当前实现重新裁决，拿到实例位置
      const gate = checkWrite(baseline, record.candidate);
      if (gate.code !== 'security_rejected') continue;
      const verdict = classifyRefusal({ candidate: record.candidate, gate });
      seen.push({ round, task: record.task, verdict: verdict.verdict, details: verdict.details });
      if (verdict.verdict === 'undetermined') undetermined.push({ round, ...verdict });
    }
  }

  // 阈值说明：这里数的是「用**当前实现**重判后仍被拒」的次数。占位修复（TF-0113）之后
  // 它从 20+ 降到 17——被降下去的那些正是占位模板，它们现在根本不会被拒。
  // 所以这个下界同时是修复效果的回归：如果哪天占位文案又被拦下来，这里会重新涨上去。
  assert.ok(seen.length >= 15, `回放到的真实拒绝太少：${seen.length}`);
  assert.deepEqual(
    undetermined,
    [],
    `分类器不该在真实拒绝上给不出结论：${JSON.stringify(undetermined.slice(0, 3))}`,
  );

  // 已知样例（人工核对过的那些）必须判对
  const byValue = (needle) =>
    seen.filter((item) => item.details.some((detail) => (detail.value ?? '').includes(needle)));

  const urlCredential = byValue('cargo-user:cargo-password');
  assert.ok(urlCredential.length > 0, '应当回放到那条真实 URL 内嵌凭据');
  assert.ok(
    urlCredential.every((item) => item.verdict === 'shape_present'),
    'URL 里真的有 user:password，必须判成形态存在（第四轮 LLM 在这里判错了）',
  );

  const ghpToken = byValue('ghp_internal_upload');
  assert.ok(ghpToken.length > 0, '应当回放到那条 ghp_ 字面量令牌');
  assert.ok(
    ghpToken.every((item) => item.verdict === 'shape_present'),
    '字面量令牌必须判成形态存在',
  );

  // 占位文案现在**根本不该再被拒**（TF-0113 的修复效果）；
  // 如果它们重新出现在拒绝集合里，说明豁免失效或引入了回归。
  assert.deepEqual(
    byValue('your-password').map((item) => item.verdict),
    [],
    '占位文案不该再被门禁拦住',
  );
  assert.deepEqual(
    byValue('REPLACE_WITH').map((item) => item.verdict),
    [],
    '带 REPLACE 的占位文案不该再被门禁拦住',
  );
});
