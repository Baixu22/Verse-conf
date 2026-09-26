#!/usr/bin/env node
/**
 * 验收脚本：WebAssembly 分发与命令行 / 本机二进制的结果必须一致。
 *
 * 对应 TF-0025 的第二条验收标准「校验与审计结果与命令行一致」，做法是
 * 用同一批语料同时跑两边，逐项比对：
 *
 *   1. `verseconf validate` 的退出码  <->  wasm `validate().valid`
 *   2. `verseconf audit --format json` 的 summary 与 rule_id 集合
 *      <->  wasm `audit()` 的 summary 与 rule_id 集合
 *   3. 同一串 JSON-RPC 请求喂给本机 `verseconf-mcp` 与 wasm 会话，
 *      逐行响应必须逐字节相同
 *
 * 唯一的例外是 `verseconf_apply_edit` 的 `inputSchema`：wasm 目标没有文件系统，
 * 因此**有意**不声明 `path`（TF-0063，用户确认过的收窄）。这一处不做逐字节比对，
 * 改成分别断言两端的声明与实际行为各自自洽：
 *
 *   - 原生：声明 `path`，`oneOf` 覆盖 source 与 path 两支，真传 path 不会落到
 *     「平台不支持」；
 *   - wasm：不声明 `path`，`oneOf` 只要求 source，真传 path 返回
 *     `unsupported_on_platform`。
 *
 * 除这一个工具外，所有响应仍然要求逐字节相同——「共用代码」不等于「能力相同」，
 * 但能力差异必须被显式写出来并被测到，而不是让比对脚本假装两端一模一样。
 *
 * 用法：
 *   node test/parity.mjs
 *   VERSECONF_CLI=/path/to/verseconf-cli VERSECONF_MCP=/path/to/verseconf-mcp node test/parity.mjs
 */

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
const here = path.dirname(fileURLToPath(import.meta.url));
const packageRoot = path.resolve(here, '..');
const repoRoot = path.resolve(packageRoot, '..', '..', '..');

const exe = (name) => (process.platform === 'win32' ? `${name}.exe` : name);
const cliBin = process.env.VERSECONF_CLI ?? path.join(repoRoot, 'target', 'debug', exe('verseconf'));
const mcpBin = process.env.VERSECONF_MCP ?? path.join(repoRoot, 'target', 'debug', exe('verseconf-mcp'));

for (const [label, binary] of [['verseconf（CLI 可执行文件）', cliBin], ['verseconf-mcp', mcpBin]]) {
  if (!existsSync(binary)) {
    console.error(`找不到 ${label}：${binary}`);
    console.error('先构建：cargo build -p verseconf-cli -p verseconf-mcp');
    process.exit(2);
  }
}

const api = require(path.join(packageRoot, 'dist', 'index.cjs'));

let checks = 0;
const mismatches = [];

function record(name, ok, detail) {
  checks += 1;
  if (ok) {
    console.log(`  ok    ${name}`);
  } else {
    mismatches.push({ name, detail });
    console.error(`  FAIL  ${name}\n        ${detail}`);
  }
}

/** 平台相关的那一个工具：wasm 有意不声明 path。 */
const PLATFORM_TOOL = 'verseconf_apply_edit';

/**
 * 把 `verseconf_apply_edit` 的 `description` 与 `inputSchema` 换成占位标记，
 * 其余原样保留。这一个工具的这两个字段都随平台变化（wasm 没有文件系统），
 * 逐字节比对只会把有意为之的差异报成回归；差异本身由下面的平台断言逐条钉住。
 * 解析失败时退回原文，不影响比对语义。
 */
function neutralizePlatformTool(jsonText) {
  try {
    const parsed = JSON.parse(jsonText);
    const visit = (value) => {
      if (Array.isArray(value)) {
        value.forEach(visit);
        return;
      }
      if (value && typeof value === 'object') {
        if (value.name === PLATFORM_TOOL) {
          if ('description' in value) {
            value.description = '<platform-dependent>';
          }
          if ('inputSchema' in value) {
            value.inputSchema = '<platform-dependent>';
          }
        }
        Object.values(value).forEach(visit);
      }
    };
    visit(parsed);
    return JSON.stringify(parsed);
  } catch {
    return jsonText;
  }
}

function collectFixtures() {
  const root = path.join(repoRoot, 'examples');
  const found = [];
  const walk = (dir) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        walk(full);
      } else if (entry.name.endsWith('.vcf')) {
        found.push(full);
      }
    }
  };
  walk(root);
  return found.sort();
}

console.log('== 语料：仓库示例（CLI 与 wasm 用同一份源码文本）');
for (const file of collectFixtures()) {
  const source = readFileSync(file, 'utf8');
  const relative = path.relative(repoRoot, file);

  // 1. 校验：CLI 退出码 vs wasm valid
  const validated = spawnSync(cliBin, ['validate', file, '--no-include'], { encoding: 'utf8' });
  const cliValid = validated.status === 0;
  const wasmValid = api.validate(source).structuredContent.valid;
  record(
    `validate ${relative}`,
    cliValid === wasmValid,
    `CLI exit=${validated.status}（valid=${cliValid}）但 wasm valid=${wasmValid}`
  );

  // 2. 审计：CLI JSON 输出 vs wasm 结构化结果
  const audited = spawnSync(cliBin, ['audit', file, '--format', 'json'], { encoding: 'utf8' });
  let cliReport;
  try {
    cliReport = JSON.parse(audited.stdout);
  } catch {
    // CLI 对无法解析的文件直接报错退出；此时只要求两边都不认为它合法
    record(
      `audit  ${relative}（CLI 无法解析）`,
      wasmValid === false,
      `CLI 解析失败但 wasm validate().valid=${wasmValid}`
    );
    continue;
  }

  const wasmReport = api.audit(source).structuredContent;
  const cliRules = cliReport.findings.map((finding) => finding.rule_id).sort();
  const wasmRules = wasmReport.findings.map((finding) => finding.rule_id).sort();
  const summaryEqual =
    cliReport.summary.total === wasmReport.summary.total &&
    cliReport.summary.critical === wasmReport.summary.critical &&
    cliReport.summary.high === wasmReport.summary.high &&
    cliReport.summary.medium === wasmReport.summary.medium &&
    cliReport.summary.low === wasmReport.summary.low &&
    cliReport.summary.info === wasmReport.summary.info;

  record(
    `audit  ${relative}`,
    summaryEqual && JSON.stringify(cliRules) === JSON.stringify(wasmRules),
    `CLI summary=${JSON.stringify(cliReport.summary)} rules=${JSON.stringify(cliRules)}；` +
      `wasm summary=${JSON.stringify(wasmReport.summary)} rules=${JSON.stringify(wasmRules)}`
  );
}

console.log('== 协议：本机 verseconf-mcp 与 wasm 会话逐字节比对');

const PLAN = {
  version: '1.0',
  edits: [{ op: 'set', path: ['server', 'port'], value: 9090, expect: { value: 8080 } }],
};
const SOURCE = '#@schema {\n  server {\n    type = "table"\n    port {\n      type = "integer"\n    }\n  }\n}\n\nserver {\n  port = 8080 #@ range(1..65535)\n}\n';

const requests = [
  { jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2024-11-05', clientInfo: { name: 'parity' } } },
  { jsonrpc: '2.0', method: 'notifications/initialized' },
  { jsonrpc: '2.0', id: 2, method: 'tools/list', params: {} },
  { jsonrpc: '2.0', id: 3, method: 'tools/call', params: { name: 'verseconf_validate', arguments: { source: SOURCE } } },
  { jsonrpc: '2.0', id: 4, method: 'tools/call', params: { name: 'verseconf_validate', arguments: { source: 'port = \n' } } },
  { jsonrpc: '2.0', id: 5, method: 'tools/call', params: { name: 'verseconf_audit', arguments: { source: 'db_password = "secret"\nhost = "0.0.0.0"\n' } } },
  { jsonrpc: '2.0', id: 6, method: 'tools/call', params: { name: 'verseconf_apply_edit', arguments: { source: SOURCE, plan: PLAN } } },
  { jsonrpc: '2.0', id: 7, method: 'tools/call', params: { name: 'verseconf_apply_edit', arguments: { source: 'port = 8080\n', plan: { version: '1.0', edits: [{ op: 'set', path: ['missing'], value: 1 }] } } } },
  { jsonrpc: '2.0', id: 8, method: 'tools/call', params: { name: 'verseconf_edit_range', arguments: { source: 'port = 8080\n', start: 0, end: 999, replacement: 'x' } } },
  { jsonrpc: '2.0', id: 9, method: 'tools/call', params: { name: 'verseconf_nope', arguments: {} } },
  { jsonrpc: '2.0', id: 10, method: 'resources/list', params: {} },
  { jsonrpc: '2.0', id: 11, method: 'ping', params: {} },
  { jsonrpc: '2.0', id: 12, method: 'tools/call', params: { arguments: {} } },
];

// 通知不产生响应：比对时必须按「会回响应的请求」对齐，否则从通知之后开始
// 每个标签都会错位一格，报出来的失败位置指向错误的请求。
const answered = requests.filter((request) => request.id !== undefined);

const lines = requests.map((request) => JSON.stringify(request));
const native = spawnSync(mcpBin, [], { input: `${lines.join('\n')}\n`, encoding: 'utf8' });
if (native.error) {
  throw native.error;
}
const nativeLines = native.stdout.split('\n').filter((line) => line.length > 0);

const session = new api.McpSession();
const wasmLines = [];
for (const line of lines) {
  const response = session.handleLine(line);
  if (response !== undefined) {
    wasmLines.push(response);
  }
}

record(
  '响应条数一致',
  nativeLines.length === wasmLines.length && nativeLines.length === answered.length,
  `原生 ${nativeLines.length} 条，wasm ${wasmLines.length} 条，应回响应 ${answered.length} 条`
);

const count = Math.min(nativeLines.length, wasmLines.length);
const toolsListIndex = answered.findIndex((request) => request.method === 'tools/list');

for (let index = 0; index < count; index += 1) {
  const request = answered[index];
  const label = `#${request.id} ${request.method}${request.params?.name ? ` ${request.params.name}` : ''}`;
  const platformDependent = request.method === 'tools/list';
  const name = platformDependent ? `${label}（apply_edit 的平台差异除外）` : label;
  const nativeText = platformDependent ? neutralizePlatformTool(nativeLines[index]) : nativeLines[index];
  const wasmText = platformDependent ? neutralizePlatformTool(wasmLines[index]) : wasmLines[index];

  if (nativeText === wasmText) {
    record(name, true);
  } else {
    // 逐字节不同时再看语义是否相同，便于定位是序列化顺序还是行为差异
    let semanticallyEqual = false;
    try {
      semanticallyEqual =
        JSON.stringify(JSON.parse(nativeText)) === JSON.stringify(JSON.parse(wasmText));
    } catch {
      semanticallyEqual = false;
    }
    record(
      name,
      false,
      `原生：${nativeLines[index]}\n        wasm：${wasmLines[index]}\n        语义相同=${semanticallyEqual}`
    );
  }
}

console.log('== 平台差异：apply_edit 的声明与实际必须各自自洽');

const toolsOf = (jsonText) => {
  const parsed = JSON.parse(jsonText);
  return parsed.result?.tools ?? parsed.tools ?? [];
};
const findApplyEdit = (tools) => tools.find((tool) => tool.name === PLATFORM_TOOL);

const nativeApplyEdit = toolsListIndex >= 0 ? findApplyEdit(toolsOf(nativeLines[toolsListIndex])) : undefined;
const wasmApplyEdit = toolsListIndex >= 0 ? findApplyEdit(toolsOf(wasmLines[toolsListIndex])) : undefined;

record(
  '原生声明 apply_edit 的 path',
  Boolean(nativeApplyEdit?.inputSchema?.properties?.path),
  `原生 apply_edit 的 properties=${JSON.stringify(Object.keys(nativeApplyEdit?.inputSchema?.properties ?? {}))}`
);
record(
  '原生 oneOf 覆盖 source 与 path 两支',
  nativeApplyEdit?.inputSchema?.oneOf?.length === 2,
  `原生 oneOf=${JSON.stringify(nativeApplyEdit?.inputSchema?.oneOf)}`
);
record(
  'wasm 不声明 apply_edit 的 path',
  wasmApplyEdit !== undefined && !wasmApplyEdit.inputSchema?.properties?.path,
  `wasm apply_edit 的 properties=${JSON.stringify(Object.keys(wasmApplyEdit?.inputSchema?.properties ?? {}))}`
);
record(
  'wasm oneOf 只要求 source',
  wasmApplyEdit?.inputSchema?.oneOf?.length === 1 &&
    JSON.stringify(wasmApplyEdit.inputSchema.oneOf[0]) === JSON.stringify({ required: ['source'] }),
  `wasm oneOf=${JSON.stringify(wasmApplyEdit?.inputSchema?.oneOf)}`
);
record(
  'wasm 的描述写明只接受 source',
  typeof wasmApplyEdit?.description === 'string' && wasmApplyEdit.description.includes('wasm'),
  `wasm apply_edit 的 description=${wasmApplyEdit?.description}`
);

// 声明与实际必须一致：真传 path 时两端的行为要跟各自的声明对得上。
const pathRequest = JSON.stringify({
  jsonrpc: '2.0',
  id: 1,
  method: 'tools/call',
  params: { name: PLATFORM_TOOL, arguments: { path: 'definitely-missing.vcf', plan: PLAN } },
});
const codeOf = (line) => {
  try {
    return JSON.parse(line).result?.structuredContent?.code;
  } catch {
    return undefined;
  }
};
const nativePathLine = spawnSync(mcpBin, [], { input: `${pathRequest}\n`, encoding: 'utf8' })
  .stdout.split('\n')
  .filter((line) => line.length > 0)[0];
const wasmPathLine = new api.McpSession().handleLine(pathRequest);
const nativePathCode = codeOf(nativePathLine);
const wasmPathCode = codeOf(wasmPathLine);

record(
  '原生传 path 不会落到「平台不支持」',
  nativePathCode !== 'unsupported_on_platform',
  `原生 code=${nativePathCode}`
);
record(
  'wasm 传 path 返回 unsupported_on_platform',
  wasmPathCode === 'unsupported_on_platform',
  `wasm code=${wasmPathCode}`
);

// `--list-tools` / `--call` 两个便捷入口也要一致
const nativeTools = spawnSync(mcpBin, ['--list-tools'], { encoding: 'utf8' }).stdout;
const wasmTools = spawnSync(process.execPath, [path.join(packageRoot, 'bin', 'verseconf-mcp-wasm.mjs'), '--list-tools'], {
  encoding: 'utf8',
}).stdout;
record(
  '--list-tools 输出一致（apply_edit 的平台差异除外）',
  neutralizePlatformTool(nativeTools) === neutralizePlatformTool(wasmTools),
  '两个入口的工具清单除 apply_edit 的平台差异外仍不一致'
);

const callArgs = JSON.stringify({ source: SOURCE });
const nativeCall = spawnSync(mcpBin, ['--call', 'verseconf_validate', callArgs], { encoding: 'utf8' });
const wasmCall = spawnSync(
  process.execPath,
  [path.join(packageRoot, 'bin', 'verseconf-mcp-wasm.mjs'), '--call', 'verseconf_validate', callArgs],
  { encoding: 'utf8' }
);
record(
  '--call 输出一致',
  nativeCall.stdout === wasmCall.stdout && nativeCall.status === wasmCall.status,
  `原生 exit=${nativeCall.status} 输出=${nativeCall.stdout}；wasm exit=${wasmCall.status} 输出=${wasmCall.stdout}`
);

console.log(`\n${checks - mismatches.length}/${checks} 项一致`);
if (mismatches.length > 0) {
  console.error(`\n${mismatches.length} 项不一致：`);
  for (const mismatch of mismatches) {
    console.error(`  - ${mismatch.name}`);
  }
  process.exit(1);
}
assert.equal(mismatches.length, 0);
console.log('WebAssembly 分发与命令行 / 本机二进制结果一致。');
