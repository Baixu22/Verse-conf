// 门禁增量实验的冻结物核验（TF-0099 / TF-0100）。
//
// 用法：
//   node benchmark/gate-increment/verify.mjs --freeze    # 开跑前生成 meta.json
//   node benchmark/gate-increment/verify.mjs --verify    # 核验有没有被人手改过
//   node benchmark/gate-increment/verify.mjs --freeze --dir <另一轮目录>
//
// `--dir` 让同一份代码服务多轮实验（第二轮与第一轮的语料/口径各自冻结、互不覆盖）。
// 不带 `--dir` 时就是本目录——第一轮的行为与加这个参数之前逐字节相同。
//
// 它钉住三件事：任务集没被改、判定口径（PREREGISTRATION.md）没被改、
// 引用的真实文档还是冻结的那一批。开跑之后这三样再变，本轮数据就作废。
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, '..', '..');

const argv = process.argv.slice(2);
const dirFlag = argv.indexOf('--dir');
const roundDir = dirFlag >= 0 && argv[dirFlag + 1] ? resolve(argv[dirFlag + 1]) : here;

const tasksPath = join(roundDir, 'tasks.json');
const preregPath = join(roundDir, 'PREREGISTRATION.md');
const metaPath = join(roundDir, 'meta.json');
const documentsRoot = join(repoRoot, 'benchmark', 'holdout', 'documents');

const sha256 = (buffer) => createHash('sha256').update(buffer).digest('hex');

function computeMeta() {
  const tasks = JSON.parse(readFileSync(tasksPath, 'utf8'));
  const documents = {};
  const clusters = new Set();
  let blocking = 0;
  let advisory = 0;

  for (const task of tasks.tasks) {
    if (documents[task.document] === undefined) {
      const file = join(documentsRoot, task.document);
      if (!existsSync(file)) {
        throw new Error(`任务 ${task.id} 引用了不存在的文档：${task.document}`);
      }
      documents[task.document] = sha256(readFileSync(file));
    }
    clusters.add(task.source_cluster);
    if (task.group === 'blocking') blocking += 1;
    else if (task.group === 'advisory') advisory += 1;
    else throw new Error(`任务 ${task.id} 的 group 非法：${task.group}`);
  }

  // 语料指纹：与 holdout 同一约定——只由「文档名 + 内容」决定，与路径无关
  const corpusFingerprint = sha256(
    Object.keys(documents)
      .sort()
      .map((name) => `${name}:${documents[name]}`)
      .join('\n'),
  );

  return {
    frozen_for: 'TF-0099 任务集 / TF-0100 预登记',
    tasks_sha256: sha256(readFileSync(tasksPath)),
    preregistration_sha256: sha256(readFileSync(preregPath)),
    corpus_fingerprint: corpusFingerprint,
    documents,
    counts: {
      tasks: tasks.tasks.length,
      documents: Object.keys(documents).length,
      clusters: clusters.size,
      blocking,
      advisory,
    },
  };
}

const mode = process.argv[2];
const current = computeMeta();

if (mode === '--freeze') {
  writeFileSync(metaPath, `${JSON.stringify(current, null, 2)}\n`);
  console.log('已冻结：');
  console.log(`  任务集   ${current.tasks_sha256.slice(0, 16)}…`);
  console.log(`  判定口径 ${current.preregistration_sha256.slice(0, 16)}…`);
  console.log(`  语料指纹 ${current.corpus_fingerprint.slice(0, 16)}…`);
  console.log(
    `  统计     ${current.counts.tasks} 任务 / ${current.counts.documents} 文档 / ` +
      `${current.counts.clusters} 来源簇 / blocking ${current.counts.blocking} / advisory ${current.counts.advisory}`,
  );
} else if (mode === '--verify') {
  if (!existsSync(metaPath)) {
    console.error('缺少 meta.json：先跑 --freeze。');
    process.exit(1);
  }
  const frozen = JSON.parse(readFileSync(metaPath, 'utf8'));
  const drift = [];
  for (const key of ['tasks_sha256', 'preregistration_sha256', 'corpus_fingerprint']) {
    if (frozen[key] !== current[key]) drift.push(`${key}：冻结 ${frozen[key]?.slice(0, 16)}… ≠ 现在 ${current[key].slice(0, 16)}…`);
  }
  if (JSON.stringify(frozen.counts) !== JSON.stringify(current.counts)) {
    drift.push(`统计变了：${JSON.stringify(frozen.counts)} → ${JSON.stringify(current.counts)}`);
  }
  const frozenDocs = Object.keys(frozen.documents ?? {}).sort();
  const currentDocs = Object.keys(current.documents).sort();
  if (JSON.stringify(frozenDocs) !== JSON.stringify(currentDocs)) {
    drift.push(`引用的文档集合变了：${frozenDocs.length} → ${currentDocs.length}`);
  } else {
    for (const name of currentDocs) {
      if (frozen.documents[name] !== current.documents[name]) {
        drift.push(`文档内容变了：${name}`);
      }
    }
  }

  if (drift.length > 0) {
    console.error('冻结物已经漂移，本轮数据作废、必须重新预登记：');
    for (const item of drift) console.error(`  - ${item}`);
    process.exit(1);
  }
  console.log('冻结物一致：任务集、判定口径与引用的真实文档都还是当初冻结的那一份。');
  console.log(`  语料指纹 ${current.corpus_fingerprint.slice(0, 16)}…`);
} else {
  console.error('用法：verify.mjs --freeze | --verify');
  process.exit(2);
}
