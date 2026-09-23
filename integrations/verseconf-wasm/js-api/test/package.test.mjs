#!/usr/bin/env node
/**
 * 发布包验收：模拟真实消费者。
 *
 * 0.1.0 的教训是「只有装一次才知道能不能用」：`npm pack` 出来的包里根本没有
 * pkg/ 目录，两条入口全部指向不存在的文件。这个脚本把当时漏掉的检查固化下来：
 *
 *   1. `npm pack` 的清单必须包含运行时真正需要的文件（wasm、glue、两个入口、bin）
 *   2. 在一个全新的空目录里安装这个 tgz
 *   3. 用 `require('verseconf')` 与 `import 'verseconf'` 各跑一遍四个能力
 *   4. 直接跑包内的零安装 stdio 服务端
 */

import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { existsSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const packageRoot = path.resolve(here, '..');

function run(command, args, options = {}) {
  return execFileSync(command, args, { encoding: 'utf8', ...options });
}

/** npm 在 Windows 上是 .cmd，交给 cmd.exe；其它进程一律不经 shell，避免路径被拆开。 */
function runNpm(args, cwd, options = {}) {
  if (process.platform === 'win32') {
    return run('cmd.exe', ['/c', 'npm', ...args], { cwd, ...options });
  }
  return run('npm', args, { cwd, ...options });
}

const REQUIRED_FILES = [
  'package.json',
  'dist/index.cjs',
  'dist/index.mjs',
  'dist/index.d.ts',
  'pkg/verseconf_wasm.js',
  'pkg/verseconf_wasm_bg.wasm',
  'bin/verseconf-mcp-wasm.mjs',
];

let passed = 0;
const failures = [];

function check(name, fn) {
  try {
    fn();
    passed += 1;
    console.log(`  ok    ${name}`);
  } catch (error) {
    failures.push({ name, error });
    console.error(`  FAIL  ${name}\n        ${error.message.split('\n')[0]}`);
  }
}

const workDir = mkdtempSync(path.join(tmpdir(), 'verseconf-pack-'));
let tarball;

try {
  console.log('== npm pack');
  const packed = JSON.parse(runNpm(['pack', '--json'], packageRoot))[0];
  tarball = path.join(packageRoot, packed.filename);
  const packedPaths = packed.files.map((file) => file.path);

  for (const required of REQUIRED_FILES) {
    check(`发布包包含 ${required}`, () => {
      assert.ok(packedPaths.includes(required), `清单里没有 ${required}；实际内容：${packedPaths.join(', ')}`);
    });
  }

  console.log('== 在干净目录安装');
  writeFileSync(
    path.join(workDir, 'package.json'),
    JSON.stringify({ name: 'consumer', version: '1.0.0', private: true }, null, 2)
  );
  runNpm(['install', tarball, '--no-audit', '--no-fund', '--ignore-scripts', '--loglevel=error'], workDir);
  check('安装后 node_modules/verseconf 存在', () => {
    assert.ok(existsSync(path.join(workDir, 'node_modules', 'verseconf', 'package.json')));
  });

  const consumerSource = `
const { validate, audit, applyEdit, editRange, tools } = require('verseconf');
const valid = validate('port = 8080\\n');
if (valid.isError !== false || valid.structuredContent.valid !== true) throw new Error('validate 结果不对');
const found = audit('db_password = "secret"\\nhost = "0.0.0.0"\\n').structuredContent.findings.map((f) => f.rule_id);
if (!found.includes('SEC-SENS-001') || !found.includes('SEC-004')) throw new Error('audit 结果不对');
const edited = applyEdit('server {\\n  port = 8080 #@ range(1..65535)\\n}\\n', {
  version: '1.0',
  edits: [{ op: 'set', path: ['server', 'port'], value: 9090 }],
});
if (!edited.structuredContent.source.includes('port = 9090')) throw new Error('applyEdit 结果不对');
if (!edited.structuredContent.source.includes('#@ range(1..65535)')) throw new Error('applyEdit 丢了元数据');
if (editRange('port = 8080\\n', 7, 11, '9090').structuredContent.source !== 'port = 9090\\n') throw new Error('editRange 结果不对');
if (tools().length !== 4) throw new Error('工具数量不对');
console.log('cjs ok');
`;
  writeFileSync(path.join(workDir, 'consumer.cjs'), consumerSource);
  check("require('verseconf') 四个能力可用", () => {
    assert.match(run(process.execPath, ['consumer.cjs'], { cwd: workDir }), /cjs ok/);
  });

  const esmSource = `
import { validate, audit, applyEdit, editRange, tools, McpSession } from 'verseconf';
const valid = validate('port = 8080\\n');
if (valid.structuredContent.valid !== true) throw new Error('validate 结果不对');
if (!audit('db_password = "secret"\\n').structuredContent.findings.some((f) => f.rule_id === 'SEC-SENS-001')) {
  throw new Error('audit 结果不对');
}
if (!applyEdit('port = 8080\\n', { version: '1.0', edits: [{ op: 'set', path: ['port'], value: 9090 }] })
  .structuredContent.source.includes('9090')) throw new Error('applyEdit 结果不对');
if (editRange('port = 8080\\n', 7, 11, '9090').structuredContent.source !== 'port = 9090\\n') throw new Error('editRange 结果不对');
if (tools().length !== 4) throw new Error('工具数量不对');
const session = new McpSession();
if (session.handleLine('{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}') === undefined) throw new Error('握手失败');
console.log('esm ok');
`;
  writeFileSync(path.join(workDir, 'consumer.mjs'), esmSource);
  check("import 'verseconf' 四个能力可用", () => {
    assert.match(run(process.execPath, ['consumer.mjs'], { cwd: workDir }), /esm ok/);
  });

  check('包内 stdio 服务端可直接运行', () => {
    const stdout = run(
      process.execPath,
      [path.join(workDir, 'node_modules', 'verseconf', 'bin', 'verseconf-mcp-wasm.mjs'), '--list-tools'],
      { cwd: workDir }
    );
    assert.equal(JSON.parse(stdout).tools.length, 4);
  });

  check('包内 stdio 服务端可完成一次真实会话', () => {
    const bin = path.join(workDir, 'node_modules', 'verseconf', 'bin', 'verseconf-mcp-wasm.mjs');
    const lines = [
      JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'initialize', params: {} }),
      JSON.stringify({
        jsonrpc: '2.0',
        id: 2,
        method: 'tools/call',
        params: { name: 'verseconf_validate', arguments: { source: 'port = 8080\n' } },
      }),
    ];
    const stdout = run(process.execPath, [bin], { cwd: workDir, input: `${lines.join('\n')}\n` });
    const responses = stdout.split('\n').filter(Boolean).map((line) => JSON.parse(line));
    assert.equal(responses.length, 2);
    assert.equal(responses[1].result.structuredContent.valid, true);
  });
} finally {
  if (tarball && existsSync(tarball)) {
    rmSync(tarball, { force: true });
  }
  rmSync(workDir, { recursive: true, force: true });
}

console.log(`\n${passed} 项通过，${failures.length} 项失败`);
if (failures.length > 0) {
  process.exit(1);
}
