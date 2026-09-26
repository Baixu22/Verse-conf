#!/usr/bin/env node
/**
 * 确认性复验的 harness（TF-0074）。
 *
 * 在**冻结**的 holdout 语料与预登记任务集上跑满预登记的次数与模型档位，
 * 把每次运行写成一行 JSONL，交给 `benchmark/analysis/cluster_report.mjs` 分析。
 *
 * 用法：
 *   # 冒烟：确定性替身，不产生任何模型调用，只验证管线
 *   node benchmark/confirmation/run.mjs --runner deterministic --out <目录>
 *
 *   # 确认性运行（消耗额度；需要网关凭据）
 *   PILOT_BASE=<接口> PILOT_API_KEY=<密钥> \
 *     node benchmark/confirmation/run.mjs --runner model \
 *       --tier glm-5.3-flash --tier <第二档位> --out <目录>
 *
 * 预登记约束（`PREREGISTRATION.md`），本 harness 逐条照做：
 *   - §四：每个（任务 × 方案 × 档位）跑满 R=3；**每次运行最多一次模型调用，不自动重试**；
 *   - §五：任何一次运行都计入分母——模型调用失败也记一行，不重试、不剔除；
 *   - §三：判定器只看原文与结果两份文本，不读被测实现的自述；
 *   - §7.2：主指标的对比基准是 `toml-edit-span`。
 *
 * 已知的协议边界（必须在结论里写明）：`toml-edit-*` 两臂走的是**点号路径**协议，
 * 表达不了 `[[key]]` 的按字段匹配定位。任务集里有 7 条这样的任务，这两臂在上面
 * 只能退化成「路径不唯一」，这是协议能力差异，不是写入方式的差异。
 */

import { execFile } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, '..', '..');
const examples = path.join(repo, 'target', 'debug', 'examples');

const R = 3; // §四：预登记次数

const ARMS = ['verseconf-intent', 'toml-edit-span', 'host-string-replace'];

const SYSTEM_PROMPT =
  '你是一个配置编辑助手。你会收到一份 TOML 配置的完整内容和一个改动要求。严格按要求做，只输出被要求的内容，不要解释、不要寒暄。';

function parseArgs(argv) {
  const args = { runner: 'deterministic', out: null, tiers: [], repeat: R, base: process.env.PILOT_BASE ?? 'http://127.0.0.1:3065/v1', apiKey: process.env.PILOT_API_KEY ?? null, reasoning: 'minimal', timeoutMs: 300000, limit: null, concurrency: 1 };
  for (let i = 0; i < argv.length; i += 1) {
    const key = argv[i];
    if (key === '--runner') args.runner = argv[++i];
    else if (key === '--out') args.out = argv[++i];
    else if (key === '--tier') args.tiers.push(argv[++i]);
    else if (key === '--repeat') args.repeat = Number(argv[++i]);
    else if (key === '--concurrency') args.concurrency = Number(argv[++i]);
    else if (key === '--base') args.base = argv[++i];
    else if (key === '--api-key') args.apiKey = argv[++i];
    else if (key === '--reasoning') args.reasoning = argv[++i];
    else if (key === '--limit') args.limit = Number(argv[++i]);
    else throw new Error(`未知参数：${key}`);
  }
  if (args.runner === 'model' && args.tiers.length === 0) {
    throw new Error('--runner model 需要至少一个 --tier（TF-0072 要求至少两个档位才能出结论）');
  }
  return args;
}

const run = (exe, args, cwd) =>
  new Promise((resolve) =>
    execFile(exe, args, { cwd, encoding: 'utf8' }, (error, stdout, stderr) =>
      resolve({ code: error ? (error.code ?? 1) : 0, stdout: stdout ?? '', stderr: stderr ?? '' })
    )
  );

// ---------------------------------------------------------------- 任务与语料

function loadTasks() {
  const spec = JSON.parse(fs.readFileSync(path.join(here, 'tasks.json'), 'utf8'));
  const documents = path.join(repo, 'benchmark', 'holdout', 'documents');
  return spec.tasks.map((task) => ({
    ...task,
    source: fs.readFileSync(path.join(documents, task.document), 'utf8'),
  }));
}

/** TOML 字面量：任务值只可能是字符串 / 整数 / 布尔 / 浮点 */
function literal(value) {
  if (typeof value === 'string') return JSON.stringify(value);
  if (typeof value === 'boolean') return value ? 'true' : 'false';
  return String(value);
}

/** 任务 id 里有引号与等号，不能直接当目录名（Windows 上非法）。 */
function safeDirName(text) {
  let hash = 0x811c9dc5;
  for (const byte of Buffer.from(text, 'utf8')) {
    hash ^= byte;
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  const slug = text.replace(/[^A-Za-z0-9._-]+/g, '_').replace(/^_+|_+$/g, '').slice(0, 80);
  return `${slug || 'task'}-${hash.toString(16).padStart(8, '0')}`;
}

/** 点号路径。对象段（[[key]] 按字段匹配）在这里退化成只写键名——见文件头的协议边界。 */
function dotted(task) {
  return task.path.map((segment) => (typeof segment === 'string' ? segment : segment.key)).join('.');
}

/** 把字节偏移换算成 JS 字符串下标（Unicode 任务上两者不相等）。 */
function byteOffsetToIndex(source, byteOffset) {
  return Buffer.from(source, 'utf8').subarray(0, byteOffset).toString('utf8').length;
}

function lineIndexOf(source, index) {
  return source.slice(0, index).split('\n').length - 1;
}

// ------------------------------------------------------------------- 判定器

/** 唯一一段连续差异 */
function singleRegion(original, result) {
  let prefix = 0;
  const max = Math.min(original.length, result.length);
  while (prefix < max && original[prefix] === result[prefix]) prefix += 1;
  let suffix = 0;
  while (
    suffix < max - prefix &&
    original[original.length - 1 - suffix] === result[result.length - 1 - suffix]
  ) {
    suffix += 1;
  }
  return {
    prefix,
    suffix,
    removed: original.slice(prefix, original.length - suffix),
    added: result.slice(prefix, result.length - suffix),
  };
}

/** §三 的口径 A（严格字节保真，7 条检查）+ 口径 B（语义正确） */
async function judge({ original, result, task, runDir }) {
  const originalLines = original.split('\n');
  const lines = result.split('\n');
  const index = lineIndexOf(original, byteOffsetToIndex(original, task.value_span[0]));
  const expected = literal(task.value);
  const originalLine = originalLines[index];

  // 原文那一行里目标值的确切文本（用 value_span 换算，避免同名值误伤）
  const targetLineStart =
    originalLines.slice(0, index).reduce((sum, line) => sum + line.length + 1, 0);
  const relStart = byteOffsetToIndex(original, task.value_span[0]) - targetLineStart;
  const relEnd = byteOffsetToIndex(original, task.value_span[1]) - targetLineStart;
  const expectedLine = originalLine.slice(0, relStart) + expected + originalLine.slice(relEnd);

  const diff = singleRegion(original, result);
  const keyPart = (line) => (line === undefined ? null : line.slice(0, line.indexOf('=')));

  // §三 的第 2、3 条按**位置**判定（开跑前修正过，见 PREREGISTRATION.md 的修正说明）：
  // 差异区间的起点必须落在目标行的键值分隔符之后。这里刻意不做子串匹配——
  // 值本身可以包含键名（pandas 的 matplotlib = "pandas:plotting._matplotlib"）
  // 或 `=`（requires-python = ">=3.11"），子串判定会把字节完全保真的结果判成不保真。
  const separatorIndex = targetLineStart + originalLine.indexOf('=');
  const changedStartsAfterSeparator = diff.prefix > separatorIndex;

  const checks = {
    one_changed_region: diff.prefix + diff.suffix + diff.removed.length === original.length,
    changed_region_excludes_key: changedStartsAfterSeparator,
    changed_region_excludes_equals: changedStartsAfterSeparator,
    line_count_unchanged: originalLines.length === lines.length,
    target_line_value_equals_expected: lines[index] === expectedLine,
    target_line_preserves_everything_but_the_value: lines[index] === expectedLine,
    target_line_key_part_unchanged:
      index < lines.length && keyPart(lines[index]) === keyPart(originalLine),
  };

  // 交叉验证：用适配层的 expect 把值读回来（口径 B）
  const planPath = path.join(runDir, 'verify.json');
  fs.writeFileSync(
    planPath,
    JSON.stringify({
      version: '1.0',
      edits: [{ op: 'set', path: task.path, value: task.value, expect: { value: task.value } }],
    })
  );
  const verify = await run(
    path.join(examples, process.platform === 'win32' ? 'apply_toml.exe' : 'apply_toml'),
    [path.join(runDir, 'config.toml'), planPath],
    repo
  );
  checks.value_reads_back = verify.code === 0;

  // §三：口径 A 是「在口径 B 之上再加七条字节检查」，所以 A 通过必须蕴含 B 通过。
  const byte_ok =
    checks.value_reads_back &&
    [
      'one_changed_region',
      'changed_region_excludes_key',
      'changed_region_excludes_equals',
      'line_count_unchanged',
      'target_line_value_equals_expected',
      'target_line_preserves_everything_but_the_value',
      'target_line_key_part_unchanged',
    ].every((name) => checks[name]);

  return { checks, semantic_ok: checks.value_reads_back, byte_ok };
}

// ------------------------------------------------------------------ 各方协议

function armPrompt(arm, task) {
  const head = `下面是配置文件的完整内容：\n\n\`\`\`toml\n${task.source}\`\`\`\n\n要求：${task.instruction}。`;
  if (arm === 'host-string-replace') {
    return `${head}\n\n有一个编辑工具接受 {"old_string": "...", "new_string": "..."}：它把文件里唯一出现的那段原文替换掉。请只输出这样一行 JSON，old_string 必须在文件里唯一出现。不要输出任何其它内容。`;
  }
  if (arm === 'toml-edit-span') {
    return `${head}\n\n有一个命令行工具接受 <文件> <点号路径> <值>，它用成熟库直接改那个值。请只输出一行 JSON：{"path": "<点号路径>", "value": <JSON 值>}，表示要传给它的路径和值。不要输出任何其它内容。`;
  }
  const shape = JSON.stringify({ version: '1.0', edits: [{ op: 'set', path: ['<字段路径>'], value: '<新值>' }] });
  return `${head}\n\n有一个工具接受「编辑意图契约」。它要一份 JSON，形状是 ${shape}（path 是从根表到目标字段的路径段数组，value 是新值）。请只输出这一份 JSON，不要输出任何其它内容。`;
}

function extractJson(text) {
  const fenced = text.match(/```(?:json)?\s*([\s\S]*?)```/);
  const body = (fenced ? fenced[1] : text).trim();
  try {
    return JSON.parse(body);
  } catch {
    /* 继续找最外层花括号 */
  }
  const start = body.indexOf('{');
  const end = body.lastIndexOf('}');
  if (start >= 0 && end > start) {
    try {
      return JSON.parse(body.slice(start, end + 1));
    } catch {
      return null;
    }
  }
  return null;
}

/** 确定性替身：不调用模型，直接给出「理想输出」，只用来验证管线。 */
function scriptedOutput(arm, task) {
  if (arm === 'host-string-replace') {
    return JSON.stringify({ old_string: task.before === undefined ? '' : String(task.before), new_string: String(task.value) });
  }
  if (arm === 'toml-edit-span') {
    return JSON.stringify({ path: dotted(task), value: task.value });
  }
  return JSON.stringify({ version: '1.0', edits: [{ op: 'set', path: task.path, value: task.value }] });
}

async function callModel(args, tier, userPrompt) {
  const headers = { 'content-type': 'application/json' };
  if (args.apiKey) headers.authorization = `Bearer ${args.apiKey}`;
  const response = await fetch(`${args.base}/responses`, {
    method: 'POST',
    headers,
    body: JSON.stringify({
      model: tier,
      instructions: SYSTEM_PROMPT,
      input: userPrompt,
      reasoning: { effort: args.reasoning },
    }),
    signal: AbortSignal.timeout(args.timeoutMs),
  });
  const text = await response.text();
  if (!response.ok) throw new Error(`HTTP ${response.status}: ${text.slice(0, 160)}`);
  const data = JSON.parse(text);
  const message = data.output?.find((item) => item.type === 'message');
  return {
    output: message?.content?.map((part) => part.text).join('') ?? data.output_text ?? '',
    input_tokens: data.usage?.input_tokens ?? 0,
    output_tokens: data.usage?.output_tokens ?? 0,
    reasoning_tokens: data.usage?.output_tokens_details?.reasoning_tokens ?? 0,
  };
}

/** 把一次输出落成对文件的改动；失败返回原因，不重试 */
async function applyOutput(arm, task, runDir, output) {
  const file = path.join(runDir, 'config.toml');
  const parsed = extractJson(output);
  if (!parsed || typeof parsed !== 'object') return { ok: false, reason: 'no_json' };

  if (arm === 'host-string-replace') {
    const { old_string: oldText, new_string: newText } = parsed;
    if (typeof oldText !== 'string' || typeof newText !== 'string') return { ok: false, reason: 'missing_old_new' };
    const source = fs.readFileSync(file, 'utf8');
    const first = source.indexOf(oldText);
    if (first < 0) return { ok: false, reason: 'old_string_not_found' };
    if (source.indexOf(oldText, first + 1) >= 0) return { ok: false, reason: 'old_string_not_unique' };
    fs.writeFileSync(file, source.slice(0, first) + newText + source.slice(first + oldText.length));
    return { ok: true };
  }

  if (arm === 'toml-edit-span') {
    if (typeof parsed.path !== 'string' || !('value' in parsed)) return { ok: false, reason: 'missing_path_value' };
    const exe = process.platform === 'win32' ? 'toml_set_span.exe' : 'toml_set_span';
    const result = await run(path.join(examples, exe), [file, parsed.path, String(parsed.value), '--write'], repo);
    // §五 要求单独统计拒绝码分布，所以把 stderr 首行原样带出来
    return { ok: result.code === 0, reason: result.code === 0 ? null : firstLine(result.stderr) };
  }

  const planPath = path.join(runDir, 'plan.json');
  fs.writeFileSync(planPath, JSON.stringify(parsed));
  const exe = process.platform === 'win32' ? 'apply_toml.exe' : 'apply_toml';
  const result = await run(path.join(examples, exe), [file, planPath, '--write'], repo);
  return { ok: result.code === 0, reason: result.code === 0 ? null : firstLine(result.stderr) };
}

function firstLine(text) {
  const line = (text ?? '').trim().split('\n')[0] ?? '';
  return line.slice(0, 160) || 'apply_failed';
}

// -------------------------------------------------------------------- 主流程

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const outDir = path.resolve(args.out ?? path.join(here, 'results'));
  fs.mkdirSync(outDir, { recursive: true });

  let tasks = loadTasks();
  if (args.limit) tasks = tasks.slice(0, args.limit);

  const tiers = args.runner === 'model' ? args.tiers : ['deterministic'];
  const jsonlPath = path.join(outDir, 'runs.jsonl');
  const stream = fs.createWriteStream(jsonlPath, { flags: 'w' });

  // 展开所有运行，再按并发度跑。每个运行彼此独立——各自一个工作目录、最多一次模型调用、
  // 不重试——所以并发只是吞吐选择，不改变 §三 / §四 / §五 的任何度量口径。
  const specs = [];
  for (const task of tasks) {
    for (const arm of ARMS) {
      for (const tier of tiers) {
        for (let repeat = 1; repeat <= args.repeat; repeat += 1) {
          specs.push({ task, arm, tier, repeat });
        }
      }
    }
  }

  let runs = 0;
  let failures = 0;
  let cursor = 0;
  const started = Date.now();

  async function runOne({ task, arm, tier, repeat }) {
    const runDir = path.join(outDir, 'work', safeDirName(`${task.id}-${arm}-${tier}-${repeat}`));
    fs.mkdirSync(runDir, { recursive: true });
    const file = path.join(runDir, 'config.toml');
    fs.writeFileSync(file, task.source);

    const record = {
      cluster: task.cluster,
      document: task.document,
      ending: task.ending,
      arm,
      model: tier,
      task: task.id,
      repeat,
      semantic_ok: false,
      byte_ok: false,
      applied: false,
      input_tokens: 0,
      output_tokens: 0,
      reasoning_tokens: 0,
      calls: 0,
      failure: null,
    };

    try {
      let output;
      if (args.runner === 'model') {
        const reply = await callModel(args, tier, armPrompt(arm, task));
        record.calls = 1;
        record.input_tokens = reply.input_tokens;
        record.output_tokens = reply.output_tokens;
        record.reasoning_tokens = reply.reasoning_tokens;
        output = reply.output;
      } else {
        output = scriptedOutput(arm, task);
      }

      const applied = await applyOutput(arm, task, runDir, output);
      record.applied = applied.ok;
      if (!applied.ok) record.failure = applied.reason;
    } catch (error) {
      // §五：模型调用失败也计入分母，不重试、不剔除
      record.failure = `model_call_failed: ${String(error).slice(0, 120)}`;
    }

    if (record.applied) {
      const result = fs.readFileSync(file, 'utf8');
      const verdict = await judge({ original: task.source, result, task, runDir });
      record.semantic_ok = verdict.semantic_ok;
      record.byte_ok = verdict.byte_ok;
    }

    if (record.failure && record.failure.startsWith('model_call_failed')) failures += 1;
    runs += 1;
    stream.write(`${JSON.stringify(record)}\n`);
    if (runs % 100 === 0) {
      const elapsed = ((Date.now() - started) / 1000).toFixed(0);
      console.log(`  ... ${runs}/${specs.length}（${elapsed}s）`);
    }
  }

  async function worker() {
    while (cursor < specs.length) {
      const spec = specs[cursor];
      cursor += 1;
      await runOne(spec);
    }
  }

  await Promise.all(
    Array.from({ length: Math.max(1, Math.min(args.concurrency, specs.length)) }, () => worker())
  );
  await new Promise((resolve) => stream.end(resolve));

  const elapsed = ((Date.now() - started) / 1000).toFixed(0);
  console.log(
    `runner=${args.runner} tiers=${tiers.join(',')} tasks=${tasks.length} arms=${ARMS.length} ` +
      `repeat=${args.repeat} concurrency=${args.concurrency}`
  );
  console.log(`写出 ${runs} 次运行到 ${jsonlPath}（模型调用失败 ${failures} 次，已计入分母；用时 ${elapsed}s）`);
  if (args.runner === 'deterministic') {
    console.log('注意：deterministic 只是管线冒烟，不是证据——结论只能用 model 模式的结果。');
  }
}

await main();
