#!/usr/bin/env node
/**
 * 真实消费者试点：Codex CLI 的 config.toml 上的三方对照。
 *
 *   node benchmark/pilot/run.mjs --document ~/.codex/config.toml [--runner deterministic|model]
 *                               [--model haiku] [--out <目录>] [--repeat N] [--only <任务 id>]
 *
 * 三方：
 *   host-edit-tool     宿主现有编辑工具（确定性替身：按字段名找第一处匹配并整行替换）
 *   toml-edit-thin     成熟保格式库的最薄语义封装（toml_edit）
 *   verseconf-intent   VerseConf 意图契约 + 字节区间最小改动
 *
 * 判定对三方完全相同，且不读被测实现的自述：
 *   1. 原文与结果之间只允许有一段改动，且必须落在目标键所在行；
 *   2. 那一段必须正好是旧值 → 新值，不得包含键名或等号（键与分隔符必须原样）；
 *   3. 其余字节逐字节相同，行数不变；
 *   4. 交叉验证：用适配层的 expect 把值读回来，必须命中。
 *
 * 红线：**只操作副本**。真实配置文件一个字节都不碰。
 *
 * `--runner model` 会逐任务调用 `claude -p`，消耗额度；默认的 deterministic
 * 只跑确定性替身，用来验证 harness 本身，不产生任何模型调用。
 */

import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {execFile} from 'node:child_process';
import {fileURLToPath} from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, '..', '..');
const examples = path.join(repo, 'target', 'debug', 'examples');

function parseArgs(argv) {
  const args = {
    runner: 'deterministic',
    repeat: 1,
    out: null,
    only: null,
    model: null,
    document: null,
    endpoint: process.env.PILOT_BASE ?? 'http://127.0.0.1:3065/v1',
    apiKey: process.env.PILOT_API_KEY ?? null,
    reasoning: 'minimal',
    timeoutMs: 120000,
    retries: 3,
  };
  for (let i = 0; i < argv.length; i += 1) {
    const key = argv[i];
    if (key === '--document') args.document = argv[++i];
    else if (key === '--runner') args.runner = argv[++i];
    else if (key === '--model') args.model = argv[++i];
    else if (key === '--endpoint') args.endpoint = argv[++i];
    else if (key === '--api-key') args.apiKey = argv[++i];
    else if (key === '--reasoning') args.reasoning = argv[++i];
    else if (key === '--timeout-ms') args.timeoutMs = Number(argv[++i]);
    else if (key === '--retries') args.retries = Number(argv[++i]);
    else if (key === '--out') args.out = argv[++i];
    else if (key === '--only') args.only = argv[++i];
    else if (key === '--repeat') args.repeat = Number(argv[++i]);
    else throw new Error(`未知参数：${key}`);
  }
  return args;
}

const run = (exe, args, cwd) => new Promise((resolve) => execFile(
  exe, args,
  {cwd, windowsHide: true, timeout: 300000, maxBuffer: 32 * 1024 * 1024},
  (error, stdout, stderr) => resolve({code: error?.code ?? 0, stdout, stderr}),
));

/** 期望值的 TOML 字面量，独立于被测实现渲染 */
function literal(value) {
  if (typeof value === 'boolean') return value ? 'true' : 'false';
  if (typeof value === 'number') return String(value);
  return `"${String(value).replace(/\\/g, '\\\\').replace(/"/g, '\\"')}"`;
}

function singleRegion(before, after) {
  let prefix = 0;
  while (prefix < before.length && prefix < after.length && before[prefix] === after[prefix]) prefix += 1;
  let suffix = 0;
  while (
    suffix < before.length - prefix &&
    suffix < after.length - prefix &&
    before[before.length - 1 - suffix] === after[after.length - 1 - suffix]
  ) suffix += 1;
  return {prefix, suffix, removed: before.slice(prefix, before.length - suffix), added: after.slice(prefix, after.length - suffix)};
}

/** 任务的目标路径段：section 是段数组，因为 TOML 的 [a.b] 是两层表 */
function segments(task) {
  return task.section ? [...task.section, task.key] : [task.key];
}

function dotted(task) {
  return segments(task).join('.');
}

const escapeRe = (text) => text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

/** 在指定小节里找目标键所在行；独立于任何 TOML 库 */
function findKeyLine(lines, task) {
  const want = (task.section ?? []).join('.');
  let section = '';
  for (let i = 0; i < lines.length; i += 1) {
    const header = lines[i].trim().match(/^\[+([^\]]+)\]+$/);
    if (header) {
      section = header[1];
      continue;
    }
    if (section !== want) continue;
    if (new RegExp(`^\\s*${escapeRe(task.key)}\\s*=`).test(lines[i])) return i;
  }
  return -1;
}

/** 独立读出「等号右边那个值」的字面量，不依赖任何 TOML 库 */
function valueTextAfterEquals(line) {
  const eq = line.indexOf('=');
  if (eq < 0) return null;
  const rest = line.slice(eq + 1).replace(/^\s+/, '');
  if (rest.startsWith('"')) {
    let out = '"';
    for (let i = 1; i < rest.length; i += 1) {
      if (rest[i] === '\\') {
        out += rest[i] + (rest[i + 1] ?? '');
        i += 1;
        continue;
      }
      out += rest[i];
      if (rest[i] === '"') return out;
    }
    return null;
  }
  if (rest.startsWith("'")) {
    const end = rest.indexOf("'", 1);
    return end < 0 ? null : rest.slice(0, end + 1);
  }
  const bare = rest.match(/^[^\s#]+/);
  return bare ? bare[0] : null;
}

async function judge({original, result, task, runDir}) {
  const diff = singleRegion(original, result);
  const lines = result.split('\n');
  const originalLines = original.split('\n');
  const index = findKeyLine(lines, task);
  const originalIndex = findKeyLine(originalLines, task);
  const expected = literal(task.value);

  const keyPart = (line) => (line === undefined ? null : line.slice(0, line.indexOf('=')));

  // 目标行除「值」以外的部分必须原样：值之前的（缩进、键名、等号、空格）
  // 与值之后的（空格、行尾注释）都要逐字节保留
  let targetLinePreserved = false;
  if (index >= 0 && originalIndex >= 0) {
    const originalLine = originalLines[originalIndex];
    const currentLiteral = valueTextAfterEquals(originalLine);
    if (currentLiteral !== null) {
      const eq = originalLine.indexOf('=');
      const at = originalLine.indexOf(currentLiteral, eq + 1);
      const expectedLine =
        originalLine.slice(0, at) + expected + originalLine.slice(at + currentLiteral.length);
      targetLinePreserved = lines[index] === expectedLine;
    }
  }

  const checks = {
    one_changed_region: diff.prefix + diff.suffix + diff.removed.length === original.length,
    changed_region_excludes_key: !diff.removed.includes(task.key),
    changed_region_excludes_equals: !diff.removed.includes('='),
    line_count_unchanged: originalLines.length === lines.length,
    // 目标行等号右边的值必须正好是要求的新值
    target_line_value_equals_expected: index >= 0 && valueTextAfterEquals(lines[index]) === expected,
    // 目标行除值以外的部分必须逐字节原样（行尾注释因此不能被动）
    target_line_preserves_everything_but_the_value: targetLinePreserved,
    // 目标行等号左边（缩进与键名）必须原样
    target_line_key_part_unchanged: index >= 0 && originalIndex >= 0 && keyPart(lines[index]) === keyPart(originalLines[originalIndex]),
  };

  // 交叉验证：用适配层的 expect 把值读回来
  const planPath = path.join(runDir, 'verify.json');
  fs.writeFileSync(planPath, JSON.stringify({
    version: '1.0',
    edits: [{op: 'set', path: segments(task), value: task.value, expect: {value: task.value}}],
  }));
  const verify = await run(path.join(examples, 'apply_toml.exe'), [path.join(runDir, 'config.toml'), planPath], repo);
  checks.value_reads_back = verify.code === 0;

  const correct = Object.values(checks).every(Boolean);
  return {correct, checks};
}

// ------------------------------------------------------------------ 三个方

const ARMS = [
  {
    id: 'host-whole-file',
    label: '宿主现状：整文件重写',
    note: '模型输出改动后的完整文件——没有专用编辑工具时的退化形态',
  },
  {
    id: 'host-string-replace',
    label: '宿主现状：字符串替换',
    note: '模型输出 old_string/new_string，宿主做朴素字符串替换——真实 Edit 工具的形状',
  },
  {
    id: 'toml-edit-thin',
    label: '成熟库薄封装（序列化写入）',
    note: 'toml_edit 的最小封装：DocumentMut → to_string()，会重新序列化整份文档',
  },
  {
    id: 'toml-edit-span',
    label: '成熟库薄封装（span 写入）',
    note: '同样的 {path,value} 协议，但用 span() 做字节替换——用来分离「协议收益」与「写入方式收益」',
  },
  {
    id: 'verseconf-intent',
    label: 'VerseConf 意图契约',
    note: '同一份编辑意图契约，字节区间最小改动 + 结果必须仍是合法 TOML',
  },
];

// ------------------------------------------- 模型协议（单轮、纯文本、无工具调用）

/** 三方共用的系统提示，保证「同模型、同任务、同预算」 */
const SYSTEM_PROMPT = '你是一个配置编辑助手。你会收到一份 TOML 配置的完整内容和一个改动要求。严格按要求做，只输出被要求的内容，不要解释、不要寒暄。';

/** 每一方的用户提示：都带同一份文件内容与同一条要求，差别只在输出协议 */
function armPrompt(arm, task, source) {
  const head = `下面是配置文件的完整内容：\n\n\`\`\`toml\n${source}\`\`\`\n\n要求：${task.instruction}。`;
  if (arm.id === 'host-whole-file') {
    return `${head}\n\n请输出改动后的完整文件，放在一个 \`\`\`toml 代码块里。不要输出任何其它内容。`;
  }
  if (arm.id === 'host-string-replace') {
    return `${head}\n\n有一个编辑工具接受 {"old_string": "...", "new_string": "..."}：它把文件里唯一出现的那段原文替换掉。请只输出这样一行 JSON，old_string 必须在文件里唯一出现。不要输出任何其它内容。`;
  }
  if (arm.id === 'toml-edit-thin' || arm.id === 'toml-edit-span') {
    // 两个臂的提示词**逐字相同**：模型输出一样，差别只在 harness 调用哪个可执行文件
    return `${head}\n\n有一个命令行工具接受 <文件> <点号路径> <值>，它用成熟库直接改那个值。请只输出一行 JSON：{"path": "<点号路径>", "value": <JSON 值>}，表示要传给它的路径和值。不要输出任何其它内容。`;
  }
  const shape = JSON.stringify({version: '1.0', edits: [{op: 'set', path: ['<字段路径>'], value: '<新值>'}]});
  return `${head}\n\n有一个工具接受「编辑意图契约」。它要一份 JSON，形状是 ${shape}（path 是从根表到目标字段的路径段数组，value 是新值）。请只输出这一份 JSON，不要输出任何其它内容。`;
}

/** 解析模型输出：容忍代码围栏与前后废话 */
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

function extractToml(text) {
  const fenced = text.match(/```(?:toml)?\s*([\s\S]*?)```/);
  return (fenced ? fenced[1] : text).replace(/^\n+/, '');
}

/** 一次模型调用：直连 OpenAI Responses 接口 */
async function callModel(args, userPrompt) {
  const headers = {'content-type': 'application/json'};
  if (args.apiKey) headers.authorization = `Bearer ${args.apiKey}`;
  const body = {
    model: args.model,
    instructions: SYSTEM_PROMPT,
    input: userPrompt,
    reasoning: {effort: args.reasoning},
  };

  // 网关对非流式响应有硬超时（实测 300 秒后返回 502），所以这里自己设一个更短的
  // 超时并重试：一次挂住不该把整轮实验带走。
  let lastError = null;
  for (let attempt = 1; attempt <= args.retries; attempt += 1) {
    const started = Date.now();
    try {
      const response = await fetch(`${args.endpoint}/responses`, {
        method: 'POST',
        headers,
        body: JSON.stringify(body),
        signal: AbortSignal.timeout(args.timeoutMs),
      });
      const text = await response.text();
      const wall_ms = Date.now() - started;
      if (!response.ok) {
        lastError = new Error(`HTTP ${response.status}: ${text.slice(0, 160)}`);
      } else {
        const data = JSON.parse(text);
        const message = data.output?.find((item) => item.type === 'message');
        return {
          output: message?.content?.map((part) => part.text).join('') ?? data.output_text ?? '',
          input_tokens: data.usage?.input_tokens ?? 0,
          output_tokens: data.usage?.output_tokens ?? 0,
          reasoning_tokens: data.usage?.output_tokens_details?.reasoning_tokens ?? 0,
          wall_ms,
          attempts: attempt,
        };
      }
    } catch (error) {
      lastError = error;
    }
    if (attempt < args.retries) {
      await new Promise((resolve) => setTimeout(resolve, 1500 * attempt));
    }
  }
  throw lastError ?? new Error('模型调用失败');
}

/** 把一次模型输出落成对文件的改动 */
async function applyModelOutput(arm, task, runDir, output) {
  const file = path.join(runDir, 'config.toml');
  if (arm.id === 'host-whole-file') {
    const rewritten = extractToml(output);
    if (!rewritten.trim()) return {ok: false, reason: '没有解析出 toml 代码块'};
    fs.writeFileSync(file, rewritten);
    return {ok: true};
  }
  const parsed = extractJson(output);
  if (!parsed || typeof parsed !== 'object') return {ok: false, reason: '没有解析出 JSON'};

  if (arm.id === 'host-string-replace') {
    const {old_string: oldText, new_string: newText} = parsed;
    if (typeof oldText !== 'string' || typeof newText !== 'string') {
      return {ok: false, reason: 'JSON 缺少 old_string 或 new_string'};
    }
    const source = fs.readFileSync(file, 'utf8');
    const first = source.indexOf(oldText);
    if (first < 0) return {ok: false, reason: 'old_string 在文件里找不到'};
    if (source.indexOf(oldText, first + 1) >= 0) return {ok: false, reason: 'old_string 在文件里出现多次'};
    fs.writeFileSync(file, source.slice(0, first) + newText + source.slice(first + oldText.length));
    return {ok: true};
  }

  if (arm.id === 'toml-edit-thin' || arm.id === 'toml-edit-span') {
    if (typeof parsed.path !== 'string' || !('value' in parsed)) {
      return {ok: false, reason: 'JSON 缺少 path 或 value'};
    }
    const exe = arm.id === 'toml-edit-thin' ? 'toml_set.exe' : 'toml_set_span.exe';
    const result = await run(path.join(examples, exe), [file, parsed.path, String(parsed.value), '--write'], repo);
    return {ok: result.code === 0, reason: result.code === 0 ? null : result.stderr.trim().slice(0, 160)};
  }

  const planPath = path.join(runDir, 'plan.json');
  fs.writeFileSync(planPath, JSON.stringify(parsed));
  const result = await run(path.join(examples, 'apply_toml.exe'), [file, planPath, '--write'], repo);
  return {ok: result.code === 0, reason: result.code === 0 ? null : result.stderr.trim().slice(0, 160)};
}

async function runArmDeterministic(arm, task, runDir, original) {
  const file = path.join(runDir, 'config.toml');
  const idle = {calls: 0, input_tokens: 0, output_tokens: 0, reasoning_tokens: 0, total_tokens: 0, wall_ms: 0};

  if (arm.id === 'host-whole-file') {
    // 朴素做法：找到目标键所在行，整行替换成 key = 新值（会吃掉行尾注释）
    const lines = original.split('\n');
    const index = findKeyLine(lines, task);
    if (index < 0) return {...idle, applied: false, reason: 'target line not found'};
    const indent = lines[index].match(/^\s*/)[0];
    lines[index] = `${indent}${task.key} = ${literal(task.value)}`;
    fs.writeFileSync(file, lines.join('\n'));
    return {...idle, applied: true};
  }

  if (arm.id === 'host-string-replace') {
    // 忠实的字符串替换：只把等号右边那个值换掉
    const lines = original.split('\n');
    const index = findKeyLine(lines, task);
    if (index < 0) return {...idle, applied: false, reason: 'target line not found'};
    const current = valueTextAfterEquals(lines[index]);
    if (current === null) return {...idle, applied: false, reason: '读不出当前值'};
    const eq = lines[index].indexOf('=');
    const at = lines[index].indexOf(current, eq + 1);
    if (at < 0) return {...idle, applied: false, reason: '定位不到当前值'};
    lines[index] = lines[index].slice(0, at) + literal(task.value) + lines[index].slice(at + current.length);
    fs.writeFileSync(file, lines.join('\n'));
    return {...idle, applied: true};
  }

  if (arm.id === 'toml-edit-thin' || arm.id === 'toml-edit-span') {
    const exe = arm.id === 'toml-edit-thin' ? 'toml_set.exe' : 'toml_set_span.exe';
    const result = await run(path.join(examples, exe), [file, dotted(task), String(task.value), '--write'], repo);
    return {...idle, applied: result.code === 0, reason: result.stderr};
  }

  const planPath = path.join(runDir, 'plan.json');
  fs.writeFileSync(planPath, JSON.stringify({version: '1.0', edits: [{op: 'set', path: segments(task), value: task.value}]}));
  const result = await run(path.join(examples, 'apply_toml.exe'), [file, planPath, '--write'], repo);
  return {...idle, applied: result.code === 0, reason: result.stderr};
}

async function runArmWithModel(arm, task, runDir, args, original) {
  const file = path.join(runDir, 'config.toml');
  fs.writeFileSync(file, original);
  const userPrompt = armPrompt(arm, task, original);

  let input_tokens = 0;
  let output_tokens = 0;
  let reasoning_tokens = 0;
  let wall_ms = 0;
  let calls = 0;
  let applied = false;
  let lastReason = null;
  let conversation = userPrompt;

  // 失败时允许一次修复重试：真实宿主就是这么做的，重试的 token 也算进成本
  for (let attempt = 1; attempt <= 2; attempt += 1) {
    const reply = await callModel(args, conversation);
    calls += 1;
    input_tokens += reply.input_tokens;
    output_tokens += reply.output_tokens;
    reasoning_tokens += reply.reasoning_tokens;
    wall_ms += reply.wall_ms;

    const outcome = await applyModelOutput(arm, task, runDir, reply.output);
    if (outcome.ok) {
      applied = true;
      break;
    }
    lastReason = outcome.reason;
    if (attempt === 1) {
      fs.writeFileSync(file, original);
      conversation = `${userPrompt}\n\n你上一次的输出没有被接受：${outcome.reason}。请只重新输出要求的内容本身。`;
    }
  }

  return {
    applied,
    calls,
    input_tokens,
    output_tokens,
    reasoning_tokens,
    total_tokens: input_tokens + output_tokens,
    wall_ms,
    error: applied ? undefined : lastReason,
  };
}

// ------------------------------------------------------------------ 主流程

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const spec = JSON.parse(fs.readFileSync(path.join(here, 'tasks.json'), 'utf8'));
  const outDir = path.resolve(args.out ?? path.join(here, 'results'));
  fs.mkdirSync(outDir, {recursive: true});

  const resolvePath = (text) =>
    text.replace(/^~/, os.homedir()).replace(/%APPDATA%/gi, process.env.APPDATA ?? '');
  const sources = {};
  for (const [key, relative] of Object.entries(spec.documents)) {
    const resolved = resolvePath(relative);
    sources[key] = {path: resolved, text: fs.readFileSync(resolved, 'utf8')};
  }

  const selected = spec.tasks.filter((task) => !args.only || task.id === args.only);
  const tasks = [];
  for (const task of selected) {
    const source = sources[task.document];
    if (!source) {
      console.log(`SKIP ${task.id}：未知文档 ${task.document}`);
      continue;
    }
    const lines = source.text.split('\n');
    const index = findKeyLine(lines, task);
    if (index < 0) {
      console.log(`SKIP ${task.id}：${task.document} 里找不到 ${dotted(task)}`);
      continue;
    }
    if (valueTextAfterEquals(lines[index]) === literal(task.value)) {
      console.log(`SKIP ${task.id}：当前值已经等于目标值，这个改动没有意义`);
      continue;
    }
    tasks.push(task);
  }
  const rows = [];

  for (const arm of ARMS) {
    for (const task of tasks) {
      const original = sources[task.document].text;
      for (let round = 1; round <= args.repeat; round += 1) {
        const runDir = path.join(outDir, 'runs', `${arm.id}__${task.id}__r${round}`);
        fs.rmSync(runDir, {recursive: true, force: true});
        fs.mkdirSync(runDir, {recursive: true});
        fs.writeFileSync(path.join(runDir, 'config.toml'), original);
        fs.writeFileSync(path.join(runDir, 'config.original.toml'), original);

        const started = Date.now();
        let execution;
        try {
          execution = args.runner === 'model'
            ? await runArmWithModel(arm, task, runDir, args, original)
            : await runArmDeterministic(arm, task, runDir, original);
        } catch (error) {
          // 一次调用失败不该把整轮实验带走：记成一次失败运行，继续跑
          execution = {
            applied: false,
            calls: 0,
            input_tokens: 0,
            output_tokens: 0,
            reasoning_tokens: 0,
            total_tokens: 0,
            error: String(error).slice(0, 200),
          };
        }
        const wall = Date.now() - started;

        const result = fs.readFileSync(path.join(runDir, 'config.toml'), 'utf8');
        const judgement = await judge({original, result, task, runDir});
        rows.push({
          arm: arm.id,
          task: task.id,
          document: task.document,
          round,
          applied: execution.applied,
          correct: judgement.correct,
          silent_mis_edit: judgement.correct ? false : execution.applied,
          checks: judgement.checks,
          model_calls: execution.calls ?? 0,
          input_tokens: execution.input_tokens ?? 0,
          output_tokens: execution.output_tokens ?? 0,
          reasoning_tokens: execution.reasoning_tokens ?? 0,
          total_tokens: execution.total_tokens ?? 0,
          wall_ms: wall,
          error: execution.error,
        });
        const mark = judgement.correct ? 'OK  ' : 'FAIL';
        console.log(`${mark} ${arm.id} / ${task.id} r${round}  tokens=${execution.total_tokens ?? 0} calls=${execution.calls ?? 0} wall=${wall}ms${execution.error ? `  ${execution.error}` : ''}`);
        // 增量落盘：跑了几十次之后崩掉不该让前面的结果全丢
        writeReport(rows, args, sources, outDir);
      }
    }
  }

  const summary = writeReport(rows, args, sources, outDir);
  console.log(`\n报告：${path.join(outDir, 'report.md')}`);
  for (const item of summary) {
    console.log(`${item.arm}: ${item.correct}/${item.runs} 正确，静默误改 ${item.silent_mis_edit}，总 token ${item.total_tokens}，每个正确任务 ${item.tokens_per_correct_task ?? '—'}`);
  }
}

/** 汇总并写出报告；每跑完一次就调用，保证中途崩掉也有结果 */
function writeReport(rows, args, sources, outDir) {
  const summary = ARMS.map((arm) => {
    const own = rows.filter((row) => row.arm === arm.id);
    const sum = (pick) => own.reduce((total, row) => total + (pick(row) ?? 0), 0);
    return {
      arm: arm.id,
      label: arm.label,
      note: arm.note,
      runs: own.length,
      correct: own.filter((row) => row.correct).length,
      silent_mis_edit: own.filter((row) => row.silent_mis_edit).length,
      model_calls: sum((row) => row.model_calls),
      input_tokens: sum((row) => row.input_tokens),
      output_tokens: sum((row) => row.output_tokens),
      reasoning_tokens: sum((row) => row.reasoning_tokens),
      total_tokens: sum((row) => row.total_tokens),
      mean_wall_ms: own.length ? Math.round(sum((row) => row.wall_ms) / own.length) : 0,
      tokens_per_correct_task: null,
    };
  });
  for (const item of summary) {
    item.tokens_per_correct_task = item.correct ? Math.round(item.total_tokens / item.correct) : null;
  }

  const report = {
    runner: args.runner,
    model: args.model,
    reasoning: args.reasoning,
    documents: Object.fromEntries(
      Object.entries(sources).map(([key, source]) => [
        key,
        {path: source.path, bytes: Buffer.byteLength(source.text, 'utf8')},
      ]),
    ),
    document_is_a_copy: true,
    tasks: new Set(rows.map((row) => row.task)).size,
    repeat: args.repeat,
    generated_at: new Date().toISOString(),
    // 每跑完一次就写一次，所以中途看报告可能是部分结果
    partial: rows.length < ARMS.length * new Set(rows.map((row) => row.task)).size * args.repeat,
    note: args.runner === 'model'
      ? `真实模型运行：${args.model}，reasoning effort 固定为 ${args.reasoning}；主指标是 token 总量（自建模型没有价格，token 量与成本成正比）`
      : '确定性替身运行：只验证 harness 本身，token/耗时都不代表真实模型，不能用作净收益结论',
    summary,
    rows,
  };
  fs.writeFileSync(path.join(outDir, 'report.json'), JSON.stringify(report, null, 2));
  fs.writeFileSync(path.join(outDir, 'report.md'), renderMarkdown(report));
  return summary;
}

function renderMarkdown(report) {
  const lines = [
    '# 真实消费者试点：真实 TOML 配置上的四方对照',
    '',
    `- 运行方式：\`${report.runner}\`${report.model ? `（模型 ${report.model}，reasoning ${report.reasoning}）` : ''}`,
    `- 文档（全部运行在副本上）：`,
    ...Object.entries(report.documents).map(([key, doc]) => `  - \`${key}\`：\`${doc.path}\`（${doc.bytes} 字节）`),
    `- 任务数：${report.tasks}，重复：${report.repeat}`,
    `- 生成时间：${report.generated_at}`,
    ...(report.partial ? ['- ⚠️ **这是部分结果**：还在跑，报告每完成一次运行就重写一次'] : []),
    '',
    `> ${report.note}`,
    '',
    '## 总览',
    '',
    '| 方 | 正确 | 静默误改 | 模型调用 | 输入 token | 输出 token | 其中推理 | 总 token | 平均耗时 | 每个正确任务 token |',
    '| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |',
  ];
  for (const item of report.summary) {
    lines.push(`| ${item.label} | ${item.correct}/${item.runs} | ${item.silent_mis_edit} | ${item.model_calls} | ${item.input_tokens} | ${item.output_tokens} | ${item.reasoning_tokens} | ${item.total_tokens} | ${item.mean_wall_ms}ms | ${item.tokens_per_correct_task ?? '—'} |`);
  }
  lines.push('', '## 判定口径', '', '三方共用同一套判定：唯一改动区间必须落在目标键所在行、正好是旧值→新值、不含键名与等号；其余字节逐字节相同、行数不变；并用适配层的 expect 把值读回来交叉验证。', '');
  lines.push('## 逐次结果', '', '| 方 | 任务 | 轮次 | 正确 | 静默误改 | 调用 | 总 token | 耗时 | 失败项 |', '| --- | --- | --- | --- | --- | --- | --- | --- | --- |');
  for (const row of report.rows) {
    const failed = Object.entries(row.checks).filter(([, ok]) => !ok).map(([name]) => name).join(', ');
    lines.push(`| ${row.arm} | ${row.task} | ${row.round} | ${row.correct ? '是' : '否'} | ${row.silent_mis_edit ? '是' : '否'} | ${row.model_calls} | ${row.total_tokens} | ${row.wall_ms}ms | ${failed || '—'} |`);
  }
  lines.push('');
  return lines.join('\n');
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
