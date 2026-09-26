#!/usr/bin/env node
// TF-0071：把预登记的三件冻结物钉上哈希，并核验它们没有漂移。
//
//   node benchmark/confirmation/preregistration.mjs --freeze   # 冻结（写 meta.json）
//   node benchmark/confirmation/preregistration.mjs --verify   # 核验（CI 用这个）
//
// 预登记的全部价值来自「在看到数据之前就把规则写死」。所以这里要能回答两个问题：
//
// 1. **任务集还是不是当初冻结的那一份？** —— 对任务数组取 SHA-256，
//    与冻结时记录的值比对；
// 2. **生成它的规则还是不是当初那一份？** —— 对生成器源码取 SHA-256。
//    生成器变了而任务集没变，说明规则改了但没人重跑；任务集变了而生成器没变，
//    说明有人手改了冻结物。两种都要报出来。
//
// 逐条「任务真的命中了它声称的特征」由 `cargo test -p verseconf-toml --test preregistration`
// 核对（那需要 TOML 解析器）；这里只管字节级与结构级的核验。

import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const workspace = join(here, '..', '..');

const PATHS = {
  tasks: join(here, 'tasks.json'),
  weights: join(here, 'weights.json'),
  meta: join(here, 'meta.json'),
  generator: join(workspace, 'crates/verseconf-toml/examples/generate_tasks.rs'),
  corpusManifest: join(workspace, 'benchmark/holdout/manifest.json'),
  // 判定口径与门槛写在这份文档里，所以它也是冻结物：改它等于改规则
  preregistration: join(here, 'PREREGISTRATION.md'),
};

/** 验收里点名要覆盖的六个特征 */
const REQUIRED_FEATURES = [
  'string',
  'integer',
  'boolean',
  'unicode',
  'trailing_comment',
  'quoted_key_path',
];

const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');

function load() {
  const tasksDocument = JSON.parse(readFileSync(PATHS.tasks, 'utf8'));
  const weights = JSON.parse(readFileSync(PATHS.weights, 'utf8'));
  const corpus = JSON.parse(readFileSync(PATHS.corpusManifest, 'utf8'));
  return {
    tasksDocument,
    weights,
    corpus,
    tasksSha: sha256(JSON.stringify(tasksDocument.tasks)),
    generatorSha: sha256(readFileSync(PATHS.generator)),
    weightsSha: sha256(readFileSync(PATHS.weights)),
    preregistrationSha: sha256(readFileSync(PATHS.preregistration)),
  };
}

/** 结构级核验：这些条件不成立时，任务集不满足 TF-0071 的验收 */
function structuralProblems({ tasksDocument, weights, corpus }) {
  const problems = [];
  const tasks = tasksDocument.tasks;

  if (!Array.isArray(tasks) || tasks.length === 0) {
    return ['tasks.json 里没有任务'];
  }

  if (tasksDocument.corpus_sha256 !== corpus.corpus_sha256) {
    problems.push(
      `tasks.json 记的语料指纹是 ${tasksDocument.corpus_sha256}，而语料清单是 ${corpus.corpus_sha256}：任务集不是在这份语料上生成的`,
    );
  }

  const seen = new Set();
  for (const task of tasks) {
    if (seen.has(task.id)) problems.push(`任务 id 重复：${task.id}`);
    seen.add(task.id);
    if (!Array.isArray(task.path) || task.path.length === 0) {
      problems.push(`${task.id}: 路径为空`);
    }
    if (task.value === undefined) problems.push(`${task.id}: 没有替换值`);
    if (!Array.isArray(task.features) || task.features.length === 0) {
      problems.push(`${task.id}: 没有声称任何特征`);
    }
    if (!task.evidence || Object.keys(task.evidence).length === 0) {
      problems.push(`${task.id}: 没有证据（特征声称必须能查）`);
    }
  }

  const perDocument = new Map();
  for (const task of tasks) {
    perDocument.set(task.document, (perDocument.get(task.document) ?? 0) + 1);
  }
  for (const document of corpus.documents) {
    const name = document.document;
    const count = perDocument.get(name) ?? 0;
    if (count < 4 || count > 6) {
      problems.push(`${name}: 有 ${count} 个任务，不在 4–6 之内`);
    }
  }
  for (const name of perDocument.keys()) {
    if (!corpus.documents.some((document) => document.document === name)) {
      problems.push(`任务集里出现了语料之外的文档：${name}`);
    }
  }

  const coverage = new Map();
  for (const task of tasks) {
    for (const feature of task.features) {
      coverage.set(feature, (coverage.get(feature) ?? 0) + 1);
    }
  }
  for (const feature of REQUIRED_FEATURES) {
    if ((coverage.get(feature) ?? 0) === 0) {
      problems.push(`必需特征 \`${feature}\` 没有任何任务覆盖`);
    }
  }

  const total = Object.values(weights).reduce((sum, value) => sum + value, 0);
  if (Math.abs(total - 1) > 1e-6) {
    problems.push(`weights.json 的权重之和是 ${total}，必须是 1`);
  }
  for (const ending of Object.keys(weights)) {
    if (ending !== 'LF' && ending !== 'CRLF') {
      problems.push(`weights.json 里有不认识的行尾类型：${ending}`);
    }
  }

  return problems;
}

function featureCoverage(tasksDocument) {
  const coverage = {};
  for (const task of tasksDocument.tasks) {
    for (const feature of task.features) {
      coverage[feature] = (coverage[feature] ?? 0) + 1;
    }
  }
  return Object.fromEntries(Object.entries(coverage).sort());
}

function main(argv) {
  const mode = argv[0];
  if (mode !== '--freeze' && mode !== '--verify') {
    console.error('用法：preregistration.mjs --freeze | --verify');
    process.exit(2);
  }

  const loaded = load();
  const problems = structuralProblems(loaded);
  if (problems.length > 0) {
    console.error('预登记不满足 TF-0071 的验收：');
    for (const problem of problems) console.error(`  - ${problem}`);
    process.exit(1);
  }

  const snapshot = {
    corpus_sha256: loaded.tasksDocument.corpus_sha256,
    tasks_sha256: loaded.tasksSha,
    generator_sha256: loaded.generatorSha,
    weights_sha256: loaded.weightsSha,
    preregistration_sha256: loaded.preregistrationSha,
    task_count: loaded.tasksDocument.tasks.length,
    document_count: loaded.corpus.documents.length,
    named_path_tasks: loaded.tasksDocument.named_path_tasks,
    feature_coverage: featureCoverage(loaded.tasksDocument),
    weights: loaded.weights,
  };

  if (mode === '--freeze') {
    writeFileSync(
      PATHS.meta,
      `${JSON.stringify({ frozen_at: new Date().toISOString(), ...snapshot }, null, 2)}\n`,
    );
    console.log(`已冻结 ${PATHS.meta}`);
  } else {
    const meta = JSON.parse(readFileSync(PATHS.meta, 'utf8'));
    const drifted = [];
    for (const key of [
      'corpus_sha256',
      'tasks_sha256',
      'generator_sha256',
      'weights_sha256',
      'preregistration_sha256',
      'task_count',
      'document_count',
      'named_path_tasks',
    ]) {
      if (JSON.stringify(meta[key]) !== JSON.stringify(snapshot[key])) {
        drifted.push(`${key}：冻结时 ${JSON.stringify(meta[key])}，现在 ${JSON.stringify(snapshot[key])}`);
      }
    }
    if (drifted.length > 0) {
      console.error('预登记已漂移（冻结之后被改过）：');
      for (const item of drifted) console.error(`  - ${item}`);
      console.error('\n规则要改就必须重新预登记，并且作废本轮已经产生的数据。');
      process.exit(1);
    }
  }

  console.log(
    `任务 ${snapshot.task_count} 个 / 文档 ${snapshot.document_count} 份 / 用 [[key]] 元素定位 ${snapshot.named_path_tasks} 个`,
  );
  console.log(`任务集指纹 ${snapshot.tasks_sha256}`);
  console.log(`生成器指纹 ${snapshot.generator_sha256}`);
  for (const [feature, count] of Object.entries(snapshot.feature_coverage)) {
    console.log(`  覆盖 ${feature.padEnd(18)} ${count}`);
  }
  if (mode === '--verify') console.log('核验通过：预登记与冻结时一致。');
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    main(process.argv.slice(2));
  } catch (error) {
    console.error(`错误：${error.message}`);
    process.exit(1);
  }
}
