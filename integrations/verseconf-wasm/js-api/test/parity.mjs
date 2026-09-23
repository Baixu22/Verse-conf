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
  nativeLines.length === wasmLines.length,
  `原生 ${nativeLines.length} 条，wasm ${wasmLines.length} 条`
);

const count = Math.min(nativeLines.length, wasmLines.length);
for (let index = 0; index < count; index += 1) {
  const label = `#${index + 1} ${requests[index].method}${requests[index].params?.name ? ` ${requests[index].params.name}` : ''}`;
  if (nativeLines[index] === wasmLines[index]) {
    record(label, true);
  } else {
    // 逐字节不同时再看语义是否相同，便于定位是序列化顺序还是行为差异
    let semanticallyEqual = false;
    try {
      semanticallyEqual = JSON.stringify(JSON.parse(nativeLines[index])) === JSON.stringify(JSON.parse(wasmLines[index]));
    } catch {
      semanticallyEqual = false;
    }
    record(
      label,
      false,
      `原生：${nativeLines[index]}\n        wasm：${wasmLines[index]}\n        语义相同=${semanticallyEqual}`
    );
  }
}

// `--list-tools` / `--call` 两个便捷入口也要一致
const nativeTools = spawnSync(mcpBin, ['--list-tools'], { encoding: 'utf8' }).stdout;
const wasmTools = spawnSync(process.execPath, [path.join(packageRoot, 'bin', 'verseconf-mcp-wasm.mjs'), '--list-tools'], {
  encoding: 'utf8',
}).stdout;
record('--list-tools 输出一致', nativeTools === wasmTools, '两个入口的工具清单不一致');

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
