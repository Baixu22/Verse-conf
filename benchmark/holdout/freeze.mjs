#!/usr/bin/env node
// TF-0070：把 holdout 语料从来源清单冻结成可核验的副本 + 清单。
//
//   node benchmark/holdout/freeze.mjs            # 生成 documents/ 与 manifest.json
//
// 冻结的含义：documents/ 里的每一份都是原文件的**逐字节副本**，manifest.json 记下
// 来源、行尾类型、字节数、行数与 SHA-256。冻结之后实验只读 documents/，
// 不再碰来源路径；任何漂移都由 verify.mjs 报出来。
//
// 这里刻意对来源做硬校验而不是「尽量成功」：行尾类型不是纯 LF/CRLF、
// 簇重复、文件缺失都会让冻结失败——因为一份带混行尾的语料会让后面的结论无法解释。

import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const sourcesPath = join(here, 'sources.json');
const documentsDir = join(here, 'documents');
const manifestPath = join(here, 'manifest.json');

const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');

function classifyEnding(bytes) {
  let crlf = 0;
  let lf = 0;
  for (let i = 0; i < bytes.length; i += 1) {
    if (bytes[i] !== 0x0a) continue;
    if (i > 0 && bytes[i - 1] === 0x0d) crlf += 1;
    else lf += 1;
  }
  const kind = crlf > 0 && lf === 0 ? 'CRLF' : crlf === 0 && lf > 0 ? 'LF' : crlf > 0 ? 'MIXED' : 'NONE';
  return { kind, crlf, lf };
}

const BARE_OR_QUOTED = /^(?:"[^"]*"|'[^']*'|[A-Za-z0-9_-]+)(?:\s*\.\s*(?:"[^"]*"|'[^']*'|[A-Za-z0-9_-]+))*$/;

/// 把一行拆成「键」与「值」两部分，引号状态跨字符跟踪。
///
/// 不能用一条正则了事：`'msvs_version' = 'auto'  # 注释` 这种单引号键会被
/// 裸键正则漏掉，而字符串里的 `#`（`url = "http://x#y"`）会被当成行尾注释。
/// 语料的能力画像要用来挑任务，数错了就会让 TF-0071 以为自己覆盖了某个特征。
function splitKeyValue(line) {
  let quote = null;
  let equalsAt = -1;
  for (let i = 0; i < line.length; i += 1) {
    const c = line[i];
    if (quote) {
      if (c === '\\' && quote === '"') i += 1;
      else if (c === quote) quote = null;
      continue;
    }
    if (c === '"' || c === "'") quote = c;
    else if (c === '=') {
      equalsAt = i;
      break;
    }
  }
  if (equalsAt < 0) return null;

  const key = line.slice(0, equalsAt).trim();
  if (key === '' || key.startsWith('#') || !BARE_OR_QUOTED.test(key)) return null;

  const rawValue = line.slice(equalsAt + 1);
  quote = null;
  let commentAt = -1;
  for (let i = 0; i < rawValue.length; i += 1) {
    const c = rawValue[i];
    if (quote) {
      if (c === '\\' && quote === '"') i += 1;
      else if (c === quote) quote = null;
      continue;
    }
    if (c === '"' || c === "'") quote = c;
    else if (c === '#') {
      commentAt = i;
      break;
    }
  }

  return {
    key,
    value: (commentAt >= 0 ? rawValue.slice(0, commentAt) : rawValue).trim(),
    trailingComment: commentAt >= 0,
  };
}

// 语料的能力画像：TF-0071 要按这些特征挑任务，所以冻结时就把它们固定下来，
// 而不是等看到结果之后再回头数。
function profile(text) {
  const counts = {
    keyValues: 0,
    strings: 0,
    integers: 0,
    floats: 0,
    booleans: 0,
    arrays: 0,
    inlineTables: 0,
    trailingComments: 0,
    quotedKeys: 0,
    multilineStrings: 0,
    arrayTables: 0,
    tables: 0,
  };
  let nonAscii = false;

  // 多行字符串会跨行；不跟踪状态的话，里面的内容会被当成一堆键值对。
  let multiline = null;

  for (const line of text.split(/\r\n|\n/)) {
    if (/[^\x00-\x7F]/.test(line)) nonAscii = true;

    if (multiline) {
      const close = line.indexOf(multiline);
      if (close >= 0) multiline = null;
      continue;
    }

    const trimmed = line.trim();
    if (trimmed === '' || trimmed.startsWith('#')) continue;
    // 表头必须整行就是一个表头。`[[0, 3], [1, 2]],` 是数组里的一行元素，
    // 只是碰巧以 `[[` 开头——按前缀判断会把它数成数组表（regex 语料里就多算了 13 个）。
    if (/^\[\[[^[\]]*\]\]\s*(?:#.*)?$/.test(trimmed)) {
      counts.arrayTables += 1;
      continue;
    }
    if (/^\[[^[\]]*\]\s*(?:#.*)?$/.test(trimmed)) {
      counts.tables += 1;
      continue;
    }

    const split = splitKeyValue(line);
    if (!split) continue;
    counts.keyValues += 1;
    if (split.key.includes('"') || split.key.includes("'")) counts.quotedKeys += 1;
    if (split.trailingComment) counts.trailingComments += 1;

    const value = split.value;
    if (value.startsWith('"""') || value.startsWith("'''")) {
      counts.multilineStrings += 1;
      const delimiter = value.slice(0, 3);
      // 同一行里闭合就不进入跨行状态
      if (value.length < 6 || value.slice(3).indexOf(delimiter) < 0) multiline = delimiter;
      counts.strings += 1;
    } else if (value.startsWith('"') || value.startsWith("'")) {
      counts.strings += 1;
    } else if (/^(true|false)$/.test(value)) {
      counts.booleans += 1;
    } else if (/^[+-]?(\d[\d_]*)$/.test(value) || /^0[xob][0-9A-Fa-f_]+$/.test(value)) {
      counts.integers += 1;
    } else if (/^[+-]?(\d[\d_]*)?\.\d[\d_]*([eE][+-]?\d+)?$/.test(value) || /^[+-]?(inf|nan)$/.test(value)) {
      counts.floats += 1;
    } else if (value.startsWith('[')) {
      counts.arrays += 1;
    } else if (value.startsWith('{')) {
      counts.inlineTables += 1;
    }
  }

  return { ...counts, nonAscii };
}

const sources = JSON.parse(readFileSync(sourcesPath, 'utf8'));
const seenClusters = new Set();
const seenNames = new Set();
const documents = [];

for (const entry of sources.documents) {
  if (seenClusters.has(entry.cluster)) {
    throw new Error(`簇重复：${entry.cluster}。每个来源只能贡献一份文档，否则独立样本数会被虚增。`);
  }
  if (seenNames.has(entry.document)) throw new Error(`文件名重复：${entry.document}`);
  seenClusters.add(entry.cluster);
  seenNames.add(entry.document);

  let bytes;
  try {
    bytes = readFileSync(entry.origin);
  } catch (error) {
    throw new Error(`来源读不到：${entry.origin}（${error.message}）`);
  }

  const { kind, crlf, lf } = classifyEnding(bytes);
  if (kind !== 'LF' && kind !== 'CRLF') {
    throw new Error(`${entry.cluster} 的行尾类型是 ${kind}（CRLF=${crlf}, LF=${lf}），语料只接受纯 LF 或纯 CRLF`);
  }

  const text = bytes.toString('utf8');
  documents.push({
    cluster: entry.cluster,
    document: entry.document,
    origin: entry.origin,
    upstream_url: entry.upstream_url ?? null,
    license_declared_in_file: entry.license_declared_in_file ?? null,
    source_note: entry.source_note ?? null,
    bytes: bytes.length,
    lines: crlf + lf,
    ending: kind,
    crlf,
    lf,
    sha256: sha256(bytes),
    features: profile(text),
  });
}

documents.sort((a, b) => (a.cluster < b.cluster ? -1 : a.cluster > b.cluster ? 1 : 0));

rmSync(documentsDir, { recursive: true, force: true });
mkdirSync(documentsDir, { recursive: true });
for (const doc of documents) {
  const entry = sources.documents.find((candidate) => candidate.cluster === doc.cluster);
  writeFileSync(join(documentsDir, doc.document), readFileSync(entry.origin));
}

// 语料指纹：只由「簇 + 文件名 + 内容哈希」决定，与来源路径无关，
// 这样把语料拷到别的机器上仍然能核验同一份语料。
const corpusFingerprint = sha256(
  Buffer.from(documents.map((doc) => `${doc.cluster}:${doc.document}:${doc.sha256}`).join('\n')),
);

const manifest = {
  version: 1,
  frozen_for: 'TF-0070 / TF-0069（确认性复验的 holdout 语料）',
  rule: '每份文档来自一个独立来源；簇数等于文档数；实验只读 documents/ 下的副本，绝不读来源路径',
  excluded_clusters: sources.excluded.clusters,
  excluded_reason: sources.excluded.reason,
  corpus_sha256: corpusFingerprint,
  counts: {
    documents: documents.length,
    clusters: new Set(documents.map((doc) => doc.cluster)).size,
    lf: documents.filter((doc) => doc.ending === 'LF').length,
    crlf: documents.filter((doc) => doc.ending === 'CRLF').length,
  },
  documents,
};

writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);

console.log(`冻结完成：${manifest.counts.documents} 份文档 / ${manifest.counts.clusters} 个来源簇`);
console.log(`  LF=${manifest.counts.lf}  CRLF=${manifest.counts.crlf}`);
console.log(`  语料指纹 ${corpusFingerprint}`);
for (const doc of documents) {
  console.log(`  ${doc.ending.padEnd(4)} ${String(doc.bytes).padStart(6)}B ${doc.document}`);
}
