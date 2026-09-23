#!/usr/bin/env node
/**
 * 加载冒烟测试：证明构建出的产物在「主流宿主环境」里真的能加载并工作。
 *
 * 覆盖四条加载路径：
 *   1. Node CommonJS：`require('verseconf')`
 *   2. Node ESM：`import 'verseconf'`
 *   3. 直接加载 wasm-bindgen 的 nodejs 目标产物
 *   4. 浏览器/打包器目标（web）用 `initSync` 同步实例化
 *
 * 外加一条：零安装的 stdio 服务端 `bin/verseconf-mcp-wasm.mjs` 能作为子进程跑起来。
 */

import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const require = createRequire(import.meta.url);
const here = path.dirname(fileURLToPath(import.meta.url));
const packageRoot = path.resolve(here, '..');

const SOURCE = '#@schema {\n  server {\n    type = "table"\n    port {\n      type = "integer"\n    }\n  }\n}\n\nserver {\n  port = 8080 #@ range(1..65535)\n}\n';
const SECRETS = 'db_password = "secret"\nhost = "0.0.0.0"\n';
const EDIT_PLAN = {
  version: '1.0',
  edits: [{ op: 'set', path: ['server', 'port'], value: 9090, expect: { value: 8080 } }],
};

let passed = 0;
const failures = [];

function check(name, fn) {
  try {
    fn();
    passed += 1;
    console.log(`  ok    ${name}`);
  } catch (error) {
    failures.push({ name, error });
    console.error(`  FAIL  ${name}`);
    console.error(`        ${error.message.split('\n')[0]}`);
  }
}

/** 四个能力在任意一条加载路径上都必须表现一致 */
function assertToolSurface(api, label) {
  const validate = api.validate(SOURCE);
  assert.equal(validate.isError, false, `${label}: validate 不应失败`);
  assert.equal(validate.structuredContent.valid, true, `${label}: 合法配置应判为 valid`);
  assert.ok(Array.isArray(validate.content) && validate.content[0].type === 'text');

  const broken = api.validate('port = \n');
  assert.equal(broken.structuredContent.valid, false, `${label}: 坏配置应判为 invalid`);
  assert.equal(broken.structuredContent.diagnostics[0].code, 'parse_error');

  const audit = api.audit(SECRETS);
  const ruleIds = audit.structuredContent.findings.map((finding) => finding.rule_id);
  assert.ok(ruleIds.includes('SEC-SENS-001'), `${label}: 应检出明文口令`);
  assert.ok(ruleIds.includes('SEC-004'), `${label}: 应检出通配绑定`);
  assert.ok(audit.structuredContent.summary.total >= 2);

  const applied = api.applyEdit(SOURCE, EDIT_PLAN);
  assert.equal(applied.isError, false, `${label}: 合法编辑不应失败`);
  assert.ok(applied.structuredContent.source.includes('port = 9090'));
  assert.ok(applied.structuredContent.source.includes('#@ range(1..65535)'), `${label}: 元数据必须保留`);
  assert.equal(applied.structuredContent.applied[0].path, 'server.port');

  const refused = api.applyEdit('port = 8080\n', {
    version: '1.0',
    edits: [{ op: 'set', path: ['missing'], value: 1 }],
  });
  assert.equal(refused.isError, true, `${label}: 目标不存在必须拒绝`);
  assert.equal(refused.structuredContent.code, 'target_not_found');

  const ranged = api.editRange('port = 8080\n', 7, 11, '9090');
  assert.equal(ranged.structuredContent.source, 'port = 9090\n');

  assert.equal(api.tools().length, 4, `${label}: 必须暴露四个工具`);
  assert.equal(api.serverInfo().name, 'verseconf');
}

console.log('== Node CommonJS（require）');
check('dist/index.cjs 可 require 且四个能力可用', () => {
  const api = require(path.join(packageRoot, 'dist', 'index.cjs'));
  assertToolSurface(api, 'cjs');
});

console.log('== Node ESM（import）');
await (async () => {
  let api;
  try {
    api = await import(pathToFileURL(path.join(packageRoot, 'dist', 'index.mjs')).href);
  } catch (error) {
    failures.push({ name: 'dist/index.mjs 可 import', error });
    console.error(`  FAIL  dist/index.mjs 可 import\n        ${error.message.split('\n')[0]}`);
    return;
  }
  check('dist/index.mjs 可 import 且四个能力可用', () => assertToolSurface(api, 'esm'));
})();

console.log('== wasm-bindgen nodejs 目标产物');
check('pkg/verseconf_wasm.js 可直接加载', () => {
  const glue = require(path.join(packageRoot, 'pkg', 'verseconf_wasm.js'));
  const envelope = JSON.parse(glue.call_tool_json('verseconf_validate', JSON.stringify({ source: SOURCE })));
  assert.equal(envelope.structuredContent.valid, true);
  assert.equal(JSON.parse(glue.tools_json()).tools.length, 4);
});

console.log('== 浏览器/打包器目标（web）');
await (async () => {
  try {
    const glue = await import(pathToFileURL(path.join(packageRoot, 'pkg-web', 'verseconf_wasm.js')).href);
    const bytes = readFileSync(path.join(packageRoot, 'pkg-web', 'verseconf_wasm_bg.wasm'));
    check('pkg-web 用 initSync 实例化后四个能力可用', () => {
      glue.initSync({ module: bytes });
      const envelope = JSON.parse(glue.call_tool_json('verseconf_audit', JSON.stringify({ source: SECRETS })));
      assert.ok(envelope.structuredContent.findings.some((finding) => finding.rule_id === 'SEC-SENS-001'));
      assert.equal(JSON.parse(glue.server_info_json()).name, 'verseconf');
    });
  } catch (error) {
    failures.push({ name: 'pkg-web 可加载', error });
    console.error(`  FAIL  pkg-web 可加载\n        ${error.message.split('\n')[0]}`);
  }
})();

console.log('== MCP 会话');
check('wasm 侧逐行 JSON-RPC 会话与原生服务端同形', () => {
  const api = require(path.join(packageRoot, 'dist', 'index.cjs'));
  const session = new api.McpSession();

  const init = JSON.parse(session.handleLine(JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'initialize', params: {} })));
  assert.equal(init.result.serverInfo.name, 'verseconf');
  assert.ok(init.result.capabilities.tools);
  assert.equal(session.isInitialized(), true);

  const list = JSON.parse(session.handleLine(JSON.stringify({ jsonrpc: '2.0', id: 2, method: 'tools/list', params: {} })));
  assert.equal(list.result.tools.length, 4);

  const notification = session.handleLine(JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' }));
  assert.equal(notification, undefined, '通知不应产生响应');
});

console.log('== 零安装 stdio 服务端');
check('bin/verseconf-mcp-wasm.mjs --list-tools 可运行', () => {
  const stdout = execFileSync(
    process.execPath,
    [path.join(packageRoot, 'bin', 'verseconf-mcp-wasm.mjs'), '--list-tools'],
    { encoding: 'utf8' }
  );
  assert.equal(JSON.parse(stdout).tools.length, 4);
});

console.log(`\n${passed} 项通过，${failures.length} 项失败`);
if (failures.length > 0) {
  process.exit(1);
}
