#!/usr/bin/env node
// TF-0073：按「运行 → 任务 → 文档 → 来源」的嵌套结构报告不确定性。
//
//   node benchmark/analysis/cluster_report.mjs --runs <runs.jsonl> --out <目录>
//   node benchmark/analysis/cluster_report.mjs --from-pilot <report.json> --out <目录>
//
// 这个脚本存在的理由是一条具体的教训：上一轮的 195 次运行**不是** 195 个独立样本。
// C1 的 12 次失败其实是 4 个任务各重复 3 次，根因集中在一份文件上——按文档聚类之后，
// 领先幅度会大幅缩水。把 N 次运行当成 N 个独立样本，就会把一份文件的特性说成普遍规律。
//
// 所以这里做三件事：
//
// 1. **独立单元逐层报数**：运行 / 任务 / 文档 / 来源四个数字并列给出，
//    而不是只给一个「N 次运行」；
// 2. **区间而不是点估计**：用 cluster bootstrap——**重抽来源簇**，
//    簇里的文档、文档里的任务、任务里的重复运行整体跟着走。独立单元是来源，
//    不是运行；
// 3. **加权汇总必须带身份**：按预登记使用分布加权的数字只能叫「加权汇总」，
//    语料与目标分布不一致时要显式说过采样，绝不能说成生产环境平均值。
//
// 结果是确定性的：随机数用固定种子（`--seed`），同一份输入重复跑得到逐字节相同的输出。

import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

export const DEFAULT_SEED = 20260926;
export const DEFAULT_ITERATIONS = 2000;
/** 语料分布与预登记使用分布差多少就算「过采样」 */
export const OVERSAMPLING_TOLERANCE = 0.1;

const ENDINGS = ['LF', 'CRLF'];
const REQUIRED_FIELDS = ['cluster', 'document', 'ending', 'arm', 'model', 'task', 'repeat'];
const REQUIRED_BOOLEANS = ['semantic_ok', 'byte_ok'];

// ————————————————————————— 输入 —————————————————————————

/** 读 JSONL：一行一次运行。任何一行不合规都直接失败，不跳过。 */
export function parseRuns(text, source = '<runs>') {
  const runs = [];
  text.split(/\r?\n/).forEach((line, index) => {
    const trimmed = line.trim();
    if (trimmed === '' || trimmed.startsWith('#')) return;
    let record;
    try {
      record = JSON.parse(trimmed);
    } catch (error) {
      throw new Error(`${source}:${index + 1} 不是合法 JSON：${error.message}`);
    }
    runs.push(validateRun(record, `${source}:${index + 1}`));
  });
  if (runs.length === 0) throw new Error(`${source}: 一行运行记录都没有`);
  return runs;
}

function validateRun(record, where) {
  for (const field of REQUIRED_FIELDS) {
    if (record[field] === undefined || record[field] === null || record[field] === '') {
      throw new Error(`${where} 缺少字段 '${field}'`);
    }
  }
  for (const field of REQUIRED_BOOLEANS) {
    if (typeof record[field] !== 'boolean') {
      throw new Error(`${where} 的 '${field}' 必须是布尔值`);
    }
  }
  if (!ENDINGS.includes(record.ending)) {
    throw new Error(`${where} 的 ending 是 '${record.ending}'，只接受 ${ENDINGS.join(' / ')}`);
  }
  return {
    cluster: String(record.cluster),
    document: String(record.document),
    ending: record.ending,
    arm: String(record.arm),
    model: String(record.model),
    task: String(record.task),
    repeat: Number(record.repeat),
    semantic_ok: record.semantic_ok,
    byte_ok: record.byte_ok,
    // 「确实改动了文件」是判定静默误改的另一半。缺省时按「至少有一个口径过了」推断，
    // 并在报告里说明这是推断出来的。
    applied: typeof record.applied === 'boolean' ? record.applied : record.semantic_ok || record.byte_ok,
    applied_inferred: typeof record.applied !== 'boolean',
  };
}

/**
 * 把上一轮试点 harness 的 report.json 转成本脚本的输入格式。
 *
 * 只用于把**探索性**数据拿出来复算，方便先验证分析口径；确认性结果必须由
 * TF-0074 的 harness 直接写出本脚本的输入格式。
 */
export function fromPilotReport(report, documentEndings = {}) {
  if (!Array.isArray(report?.rows)) throw new Error('report.json 里没有 rows 数组');
  return report.rows.map((row, index) => {
    const where = `report.json rows[${index}]`;
    return validateRun(
      {
        cluster: row.document,
        document: row.document,
        ending: documentEndings[row.document] ?? 'UNKNOWN',
        arm: row.arm,
        model: report.model ?? 'unknown',
        task: row.task,
        repeat: row.round,
        semantic_ok: Boolean(row.checks?.value_reads_back),
        byte_ok: Boolean(row.correct),
        applied: Boolean(row.applied),
      },
      where,
    );
  });
}

// ————————————————————— 结构：四个层级的独立单元 —————————————————————

export function countUnits(runs) {
  const clusters = new Set();
  const documents = new Set();
  const tasks = new Set();
  for (const run of runs) {
    clusters.add(run.cluster);
    documents.add(run.document);
    tasks.add(`${run.cluster}\u0000${run.document}\u0000${run.task}`);
  }
  return {
    runs: runs.length,
    tasks: tasks.size,
    documents: documents.size,
    clusters: clusters.size,
  };
}

// ————————————————————————— 统计 —————————————————————————

const METRICS = {
  semantic: (run) => run.semantic_ok,
  byte: (run) => run.byte_ok,
};

function rate(runs, pick) {
  if (runs.length === 0) return null;
  const passed = runs.filter(pick).length;
  return { n: runs.length, passed, rate: passed / runs.length };
}

/** 可复现的伪随机数：不用 Math.random，否则同一份输入两次跑出不同区间 */
function mulberry32(seed) {
  let state = seed >>> 0;
  return function next() {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function percentile(sorted, q) {
  if (sorted.length === 0) return null;
  const index = (sorted.length - 1) * q;
  const low = Math.floor(index);
  const high = Math.ceil(index);
  if (low === high) return sorted[low];
  return sorted[low] + (sorted[high] - sorted[low]) * (index - low);
}

/**
 * cluster bootstrap：**重抽来源簇**，簇内部的文档/任务/重复运行整体跟着走。
 *
 * 这是这个脚本的核心。独立单元是来源，不是运行；把运行当成独立单元重抽，
 * 区间会窄得离谱，正好掩盖「优势集中在 1 份文件」这类事实。
 */
export function clusterBootstrap(runs, pick, { iterations = DEFAULT_ITERATIONS, seed = DEFAULT_SEED, weightFor } = {}) {
  const byCluster = new Map();
  for (const run of runs) {
    if (!byCluster.has(run.cluster)) byCluster.set(run.cluster, []);
    byCluster.get(run.cluster).push(run);
  }
  const clusters = [...byCluster.keys()];
  const point = rate(runs, pick);
  if (clusters.length < 2) {
    // 只有一个来源时，「按来源聚类」这件事无从谈起：不给区间，直接说不能给。
    // 每一份文档都是它自己那一簇，所以按文档分组的区间本来就必然不可用——
    // 这正是这个脚本要指出来的事实，而不是要绕过去的麻烦。
    return { point, low: null, high: null, iterations: 0, usable: false, reason: '只有 1 个来源，无法按来源聚类' };
  }

  const random = mulberry32(seed);
  const samples = [];
  let emptyStrata = 0;

  for (let i = 0; i < iterations; i += 1) {
    const drawn = [];
    for (let j = 0; j < clusters.length; j += 1) {
      const index = Math.floor(random() * clusters.length);
      drawn.push(...byCluster.get(clusters[index]));
    }
    if (weightFor) {
      const value = weightFor(drawn);
      if (value === null) emptyStrata += 1;
      else samples.push(value);
    } else {
      const estimate = rate(drawn, pick);
      if (estimate) samples.push(estimate.rate);
    }
  }

  samples.sort((a, b) => a - b);
  return {
    point,
    low: percentile(samples, 0.025),
    high: percentile(samples, 0.975),
    median: percentile(samples, 0.5),
    iterations: samples.length,
    usable: true,
    emptyStrata,
  };
}

function groupBy(runs, keyOf) {
  const groups = new Map();
  for (const run of runs) {
    const key = keyOf(run);
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key).push(run);
  }
  return groups;
}

/** 每个分组各自给出点估计 + cluster bootstrap 区间 */
export function stratified(runs, keyOf, options = {}) {
  const rows = [];
  for (const [key, group] of [...groupBy(runs, keyOf)].sort((a, b) => (a[0] < b[0] ? -1 : 1))) {
    rows.push({
      key,
      units: countUnits(group),
      semantic: clusterBootstrap(group, METRICS.semantic, options),
      byte: clusterBootstrap(group, METRICS.byte, options),
    });
  }
  return rows;
}

/**
 * 按预登记使用分布加权。
 *
 * 加权汇总的前提是**权重来自预登记**，不是从语料里量出来的。所以这里要求调用方
 * 显式给权重；没给就不出加权汇总，而不是拿语料自己的比例冒充。
 */
export function weightedSummary(runs, weights, options = {}) {
  const endings = Object.keys(weights);
  const total = endings.reduce((sum, ending) => sum + weights[ending], 0);
  if (Math.abs(total - 1) > 1e-6) {
    throw new Error(`权重之和必须是 1，实际是 ${total}`);
  }

  const observed = {};
  for (const ending of endings) {
    const own = runs.filter((run) => run.ending === ending);
    observed[ending] = { units: countUnits(own), share: own.length / runs.length };
  }

  const mismatch = Math.max(...endings.map((ending) => Math.abs(observed[ending].share - weights[ending])));
  const oversampled = mismatch > OVERSAMPLING_TOLERANCE;

  const weightedRate = (subset, pick) => {
    let value = 0;
    let usedWeight = 0;
    for (const ending of endings) {
      const own = subset.filter((run) => run.ending === ending);
      if (own.length === 0) continue;
      value += weights[ending] * (own.filter(pick).length / own.length);
      usedWeight += weights[ending];
    }
    // 某个层在重抽里空了：把权重在非空层上重新归一，并把它记下来。
    return usedWeight === 0 ? null : value / usedWeight;
  };

  const point = (pick) => weightedRate(runs, pick);

  const bootstrap = (pick) => {
    const byCluster = new Map();
    for (const run of runs) {
      if (!byCluster.has(run.cluster)) byCluster.set(run.cluster, []);
      byCluster.get(run.cluster).push(run);
    }
    const clusters = [...byCluster.keys()];
    const random = mulberry32(options.seed ?? DEFAULT_SEED);
    const samples = [];
    for (let i = 0; i < (options.iterations ?? DEFAULT_ITERATIONS); i += 1) {
      const drawn = [];
      for (let j = 0; j < clusters.length; j += 1) {
        drawn.push(...byCluster.get(clusters[Math.floor(random() * clusters.length)]));
      }
      const value = weightedRate(drawn, pick);
      if (value !== null) samples.push(value);
    }
    samples.sort((a, b) => a - b);
    return { low: percentile(samples, 0.025), high: percentile(samples, 0.975) };
  };

  const build = (pick) => {
    const interval = bootstrap(pick);
    return { rate: point(pick), low: interval.low, high: interval.high };
  };

  return {
    weights,
    weights_source: options.weightsSource ?? '未注明',
    observed,
    oversampled,
    max_deviation: mismatch,
    // 加权汇总**永远**不是生产环境平均值：权重是一个假设，不是一个测量结果。
    is_production_average: false,
    label: oversampled
      ? '按预登记使用分布加权（语料与目标分布不一致，**不是**生产环境平均值）'
      : '按预登记使用分布加权（权重是假设，不是测量结果）',
    semantic: build(METRICS.semantic),
    byte: build(METRICS.byte),
  };
}

// ————————————————————————— 报告 —————————————————————————

export function buildReport(runs, options = {}) {
  const units = countUnits(runs);
  const appliedInferred = runs.filter((run) => run.applied_inferred).length;

  const report = {
    generated_from: options.source ?? '<runs>',
    seed: options.seed ?? DEFAULT_SEED,
    iterations: options.iterations ?? DEFAULT_ITERATIONS,
    // 这四个数字必须并排出现：N 次运行不是 N 个独立样本。
    units,
    independence_note: `独立单元是**来源**：${units.runs} 次运行来自 ${units.tasks} 个任务、${units.documents} 份文档、${units.clusters} 个来源。把 ${units.runs} 当成独立样本数会高估证据量。`,
    overall: {
      semantic: clusterBootstrap(runs, METRICS.semantic, options),
      byte: clusterBootstrap(runs, METRICS.byte, options),
    },
    by_document: stratified(runs, (run) => `${run.document} [${run.ending}]`, options),
    by_ending: stratified(runs, (run) => run.ending, options),
    by_model: stratified(runs, (run) => run.model, options),
    by_arm: stratified(runs, (run) => run.arm, options),
  };

  if (appliedInferred > 0) {
    report.applied_inferred_runs = appliedInferred;
    report.applied_note = `${appliedInferred} 次运行的 applied 是推断出来的（输入里没给），静默误改的统计因此不完全可靠`;
  }

  const silent = runs.filter((run) => run.applied && !run.byte_ok);
  report.silent_miswrites = { runs: silent.length, documents: countUnits(silent).documents, clusters: countUnits(silent).clusters };

  report.weighted = options.weights ? weightedSummary(runs, options.weights, options) : null;
  if (!report.weighted) {
    report.weighted_note = '未给预登记使用分布（--weights），所以不出加权汇总——用语言里的比例冒充使用分布，正是这一节要避免的事';
  }

  return report;
}

const fmt = (value) => (value === null || value === undefined ? '—' : `${(value * 100).toFixed(1)}%`);

function interval(estimate) {
  const point = estimate.point ? fmt(estimate.point.rate) : '—';
  // 区间不可用时也要把点估计报出来（不然按文档那张表就没信息了），
  // 但必须当场说明为什么没有区间，而不是悄悄退化成一个看起来精确的数字。
  if (!estimate.usable) return `${point}（区间不可用：${estimate.reason}）`;
  return `${point} [${fmt(estimate.low)}, ${fmt(estimate.high)}]`;
}

export function renderMarkdown(report) {
  const lines = [
    '# 按来源聚类的不确定性报告（TF-0073）',
    '',
    `来源：\`${report.generated_from}\`　种子：${report.seed}　重抽次数：${report.iterations}`,
    '',
    '## 一、独立单元',
    '',
    '| 层级 | 数量 |',
    '| --- | --- |',
    `| 运行 | ${report.units.runs} |`,
    `| 任务 | ${report.units.tasks} |`,
    `| 文档 | ${report.units.documents} |`,
    `| **来源** | **${report.units.clusters}** |`,
    '',
    report.independence_note,
    '',
    '区间是 cluster bootstrap（重抽来源簇，簇内部整体跟着走）的 95% 百分位区间。',
    '',
    '## 二、总体',
    '',
    '| 口径 | 通过率 [95% 区间] |',
    '| --- | --- |',
    `| 语义正确 | ${interval(report.overall.semantic)} |`,
    `| 字节保真 | ${interval(report.overall.byte)} |`,
    '',
  ];

  const tables = [
    ['三、按文档', report.by_document],
    ['四、按行尾类型', report.by_ending],
    ['五、按模型档位', report.by_model],
    ['六、按方案', report.by_arm],
  ];
  for (const [title, rows] of tables) {
    lines.push(`## ${title}`, '', '| 分组 | 运行 | 任务 | 来源 | 语义正确 | 字节保真 |', '| --- | --- | --- | --- | --- | --- |');
    for (const row of rows) {
      lines.push(
        `| ${row.key} | ${row.units.runs} | ${row.units.tasks} | ${row.units.clusters} | ${interval(row.semantic)} | ${interval(row.byte)} |`,
      );
    }
    lines.push('');
  }

  lines.push('## 七、静默误改', '', `| 项 | 数量 |`, '| --- | --- |', `| 次数 | ${report.silent_miswrites.runs} |`, `| 涉及文档 | ${report.silent_miswrites.documents} |`, `| 涉及来源 | ${report.silent_miswrites.clusters} |`, '');

  lines.push('## 八、加权汇总', '');
  if (!report.weighted) {
    lines.push(report.weighted_note, '');
  } else {
    const w = report.weighted;
    lines.push(`**${w.label}**`, '', `权重来源：${w.weights_source}`, '', '| 行尾 | 预登记权重 | 语料实际占比 |', '| --- | --- | --- |');
    for (const ending of Object.keys(w.weights)) {
      lines.push(`| ${ending} | ${fmt(w.weights[ending])} | ${fmt(w.observed[ending].share)} |`);
    }
    lines.push('', `| 口径 | 加权通过率 [95% 区间] |`, '| --- | --- |', `| 语义正确 | ${fmt(w.semantic.rate)} [${fmt(w.semantic.low)}, ${fmt(w.semantic.high)}] |`, `| 字节保真 | ${fmt(w.byte.rate)} [${fmt(w.byte.low)}, ${fmt(w.byte.high)}] |`, '');
    if (w.oversampled) {
      lines.push(
        `> **过采样警告**：语料的行尾分布与预登记使用分布最大差 ${fmt(w.max_deviation)}，` +
          '所以上面的加权数字是**按假设分布重加权**的结果，**不能**说成生产环境平均值。' +
          '它是「如果实际使用分布确实是这个权重，结论会是什么样」，权重本身需要预登记，不是从这批语料量出来的。',
        '',
      );
    }
  }

  if (report.applied_note) lines.push(`> ${report.applied_note}`, '');

  return `${lines.join('\n')}\n`;
}

// ————————————————————————— CLI —————————————————————————

function parseArgs(argv) {
  const args = {
    runs: null,
    fromPilot: null,
    out: null,
    weights: null,
    seed: DEFAULT_SEED,
    iterations: DEFAULT_ITERATIONS,
    documentEndings: null,
  };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--runs') args.runs = argv[++i];
    else if (arg === '--from-pilot') args.fromPilot = argv[++i];
    else if (arg === '--out') args.out = argv[++i];
    else if (arg === '--weights') args.weights = argv[++i];
    else if (arg === '--document-endings') args.documentEndings = argv[++i];
    else if (arg === '--seed') args.seed = Number(argv[++i]);
    else if (arg === '--iterations') args.iterations = Number(argv[++i]);
    else throw new Error(`不认识的参数：${arg}`);
  }
  return args;
}

export function loadInputs(args) {
  if (args.runs && args.fromPilot) throw new Error('--runs 与 --from-pilot 只能给一个');
  if (!args.runs && !args.fromPilot) throw new Error('必须给 --runs 或 --from-pilot');

  let runs;
  let source;
  if (args.runs) {
    source = args.runs;
    runs = parseRuns(readFileSync(args.runs, 'utf8'), args.runs);
  } else {
    source = args.fromPilot;
    const report = JSON.parse(readFileSync(args.fromPilot, 'utf8'));
    const endings = args.documentEndings ? JSON.parse(readFileSync(args.documentEndings, 'utf8')) : {};
    runs = fromPilotReport(report, endings);
  }

  const weights = args.weights ? JSON.parse(readFileSync(args.weights, 'utf8')) : null;
  return { runs, source, weights };
}

function main(argv) {
  const args = parseArgs(argv);
  const { runs, source, weights } = loadInputs(args);

  const report = buildReport(runs, {
    source,
    seed: args.seed,
    iterations: args.iterations,
    weights,
    weightsSource: args.weights ?? '未提供',
  });

  const markdown = renderMarkdown(report);
  if (args.out) {
    mkdirSync(args.out, { recursive: true });
    writeFileSync(join(args.out, 'cluster-report.json'), `${JSON.stringify(report, null, 2)}\n`);
    writeFileSync(join(args.out, 'cluster-report.md'), markdown);
    console.log(`报告：${join(args.out, 'cluster-report.md')}`);
  } else {
    process.stdout.write(markdown);
  }

  // 让下游能核验「这份报告对应的是哪一批运行」
  const fingerprint = createHash('sha256').update(JSON.stringify(runs)).digest('hex');
  console.log(`输入指纹 ${fingerprint}`);
}

const invokedDirectly =
  process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href;
if (invokedDirectly) {
  try {
    main(process.argv.slice(2));
  } catch (error) {
    console.error(`错误：${error.message}`);
    process.exit(1);
  }
}
