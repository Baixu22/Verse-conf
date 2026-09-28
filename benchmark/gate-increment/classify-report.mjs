import { existsSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { classifyRefusal } from './classify.mjs';
import { checkWrite } from './run.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..', '..');
const documentsRoot = join(repo, 'benchmark', 'holdout', 'documents');
const tasks = JSON.parse(readFileSync(join(here, 'tasks.json'), 'utf8')).tasks;
const rounds = [
  ['第一轮', 'benchmark/gate-increment'],
  ['第二轮', 'benchmark/gate-increment-round2'],
  ['第三轮', 'benchmark/gate-increment-round3'],
  ['第四轮', 'benchmark/gate-increment-round4'],
];

console.log('回顾性统计：用确定性分类器复核四轮里被拒的候选（不改变任何一轮的裁决）');
console.log('口径说明：本脚本用**当前实现**重新裁决历史候选（因为前四轮跑的时候实例位置还没被存下来），');
console.log('所以"拒绝 N 次"是"在当前实现下仍被拒的次数"，不等于该轮当时记录的拒绝次数。\n');

for (const [label, round] of rounds) {
  const runsPath = join(repo, round, 'results', 'runs.jsonl');
  if (!existsSync(runsPath)) continue;
  const records = readFileSync(runsPath, 'utf8').split('\n').filter((l) => l.trim()).map((l) => JSON.parse(l));
  const counts = { shape_present: 0, shape_absent: 0, undetermined: 0 };
  const absentDetail = [];
  let refusals = 0;

  for (const record of records) {
    if (!record.refused || !record.candidate) continue;
    const task = tasks.find((t) => t.id === record.task);
    const baseline = readFileSync(join(documentsRoot, task.document), 'utf8');
    const gate = checkWrite(baseline, record.candidate);
    if (gate.code !== 'security_rejected') continue;
    refusals += 1;
    const verdict = classifyRefusal({ candidate: record.candidate, gate });
    counts[verdict.verdict] += 1;
    if (verdict.verdict === 'shape_absent') {
      absentDetail.push(
        `${record.task}: ` + verdict.details.map((d) => `${d.rule}(=${d.value ?? '?'})`).join(', '),
      );
    }
  }

  const share = refusals > 0 ? ((counts.shape_absent / refusals) * 100).toFixed(0) : '—';
  console.log(`${label}：拒绝 ${refusals} 次 → 形态存在 ${counts.shape_present}、实现假阳性 ${counts.shape_absent}（${share}%）、无法判定 ${counts.undetermined}`);
  for (const line of absentDetail) console.log(`    实现假阳性 · ${line}`);
}
