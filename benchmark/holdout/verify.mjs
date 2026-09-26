#!/usr/bin/env node
// TF-0070：核验冻结后的 holdout 语料没有被改动，并逐条检查验收口径。
//
//   node benchmark/holdout/verify.mjs
//
// 退出码 0 = 语料与 manifest.json 完全一致且满足 TF-0070 的验收条件。
// 任何漂移都会以非 0 退出并打印具体是哪一份、差在哪。

import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const manifest = JSON.parse(readFileSync(join(here, 'manifest.json'), 'utf8'));
const documentsDir = join(here, 'documents');

const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');

function classifyEnding(bytes) {
  let crlf = 0;
  let lf = 0;
  for (let i = 0; i < bytes.length; i += 1) {
    if (bytes[i] !== 0x0a) continue;
    if (i > 0 && bytes[i - 1] === 0x0d) crlf += 1;
    else lf += 1;
  }
  return { kind: crlf > 0 && lf === 0 ? 'CRLF' : crlf === 0 && lf > 0 ? 'LF' : crlf > 0 ? 'MIXED' : 'NONE', crlf, lf };
}

const problems = [];
const note = (message) => problems.push(message);

const onDisk = readdirSync(documentsDir).sort();
const expected = manifest.documents.map((doc) => doc.document).sort();
if (onDisk.join('\n') !== expected.join('\n')) {
  note(`documents/ 下的文件与 manifest 不一致：\n  实际 ${onDisk.join(', ')}\n  期望 ${expected.join(', ')}`);
}

for (const doc of manifest.documents) {
  const bytes = readFileSync(join(documentsDir, doc.document));
  const actual = sha256(bytes);
  if (actual !== doc.sha256) note(`${doc.document}: SHA-256 漂移（期望 ${doc.sha256}，实际 ${actual}）`);
  if (bytes.length !== doc.bytes) note(`${doc.document}: 字节数漂移（期望 ${doc.bytes}，实际 ${bytes.length}）`);
  const ending = classifyEnding(bytes);
  if (ending.kind !== doc.ending) note(`${doc.document}: 行尾类型漂移（期望 ${doc.ending}，实际 ${ending.kind}）`);
  if (ending.crlf !== doc.crlf || ending.lf !== doc.lf) {
    note(`${doc.document}: 换行计数漂移（期望 CRLF=${doc.crlf}/LF=${doc.lf}，实际 CRLF=${ending.crlf}/LF=${ending.lf}）`);
  }
}

const fingerprint = sha256(
  Buffer.from(
    [...manifest.documents]
      .sort((a, b) => (a.cluster < b.cluster ? -1 : a.cluster > b.cluster ? 1 : 0))
      .map((doc) => `${doc.cluster}:${doc.document}:${doc.sha256}`)
      .join('\n'),
  ),
);
if (fingerprint !== manifest.corpus_sha256) {
  note(`语料指纹漂移（期望 ${manifest.corpus_sha256}，实际 ${fingerprint}）`);
}

// —— TF-0070 验收条件 ——
const clusters = new Set(manifest.documents.map((doc) => doc.cluster));
const lf = manifest.documents.filter((doc) => doc.ending === 'LF').length;
const crlf = manifest.documents.filter((doc) => doc.ending === 'CRLF').length;

if (manifest.documents.length < 12 || manifest.documents.length > 16) {
  note(`文档数 ${manifest.documents.length} 不在 12–16 之内`);
}
if (clusters.size !== manifest.documents.length) {
  note(`簇数 ${clusters.size} 少于文档数 ${manifest.documents.length}：有来源被当成了多个独立样本`);
}
if (clusters.size < 8) note(`簇数 ${clusters.size} 少于 8`);
if (lf < 6) note(`LF 只有 ${lf} 份，少于 6`);
if (crlf < 6) note(`CRLF 只有 ${crlf} 份，少于 6`);

for (const forbidden of manifest.excluded_clusters) {
  if (clusters.has(forbidden)) note(`语料里出现了已用于探索性实验的来源：${forbidden}`);
}

console.log(`文档 ${manifest.documents.length} 份 / 来源簇 ${clusters.size} 个 / LF ${lf} 份 / CRLF ${crlf} 份`);
console.log(`语料指纹 ${manifest.corpus_sha256}`);

if (problems.length > 0) {
  console.error('\n核验失败：');
  for (const problem of problems) console.error(`  - ${problem}`);
  process.exit(1);
}
console.log('核验通过：语料与 manifest.json 完全一致，且满足 TF-0070 的验收条件。');
