#!/usr/bin/env node
/**
 * 语言服务器路径解析的单元测试。
 *
 * 旧实现只写死 `server/bin/verseconf-lsp.exe`，等于只支持 Windows。
 * 这里固定住「按 <platform>-<arch> 解析 + 旧布局兜底 + 找不到就明确返回 undefined」
 * 这三条行为，避免以后再退回单平台。
 */

import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const here = path.dirname(fileURLToPath(import.meta.url));
const extensionRoot = path.resolve(here, '..');
const { ensureExecutable, findServerBinary, serverBinaryRelativePath } = require(
  path.join(extensionRoot, 'out', 'serverPath.js')
);

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

const root = mkdtempSync(path.join(tmpdir(), 'verseconf-ext-'));
const place = (relative) => {
  const absolute = path.join(root, ...relative.split('/'));
  mkdirSync(path.dirname(absolute), { recursive: true });
  writeFileSync(absolute, 'stub');
  return absolute;
};

try {
  const winBinary = place('server/bin/win32-x64/verseconf-lsp.exe');
  const linuxBinary = place('server/bin/linux-x64/verseconf-lsp');

  check('相对路径按 <platform>-<arch> 生成', () => {
    assert.equal(serverBinaryRelativePath('win32', 'x64'), 'server/bin/win32-x64/verseconf-lsp.exe');
    assert.equal(serverBinaryRelativePath('linux', 'arm64'), 'server/bin/linux-arm64/verseconf-lsp');
    assert.equal(serverBinaryRelativePath('darwin', 'arm64'), 'server/bin/darwin-arm64/verseconf-lsp');
  });

  check('Windows 解析到 win32-x64 的 .exe', () => {
    assert.equal(findServerBinary(root, 'win32', 'x64'), winBinary);
  });

  check('Linux 解析到 linux-x64 的无扩展名二进制', () => {
    assert.equal(findServerBinary(root, 'linux', 'x64'), linuxBinary);
  });

  check('没有对应平台目录时返回 undefined，而不是猜一个路径', () => {
    assert.equal(findServerBinary(root, 'darwin', 'arm64'), undefined);
    assert.equal(findServerBinary(root, 'win32', 'arm64'), undefined);
  });

  check('旧布局（server/bin 下直接放二进制）仍可兜底', () => {
    const legacy = place('server/bin/verseconf-lsp');
    assert.equal(findServerBinary(root, 'darwin', 'arm64'), legacy);
  });

  check('完全没有二进制时返回 undefined', () => {
    const empty = mkdtempSync(path.join(tmpdir(), 'verseconf-ext-empty-'));
    try {
      assert.equal(findServerBinary(empty, 'linux', 'x64'), undefined);
    } finally {
      rmSync(empty, { recursive: true, force: true });
    }
  });

  check('ensureExecutable 在缺失文件上不抛异常（只读文件系统也不该中断激活）', () => {
    assert.doesNotThrow(() => ensureExecutable(path.join(root, 'does-not-exist'), 'linux'));
    assert.doesNotThrow(() => ensureExecutable(linuxBinary, 'win32'));
  });
} finally {
  rmSync(root, { recursive: true, force: true });
}

console.log(`\n${passed} 项通过，${failures.length} 项失败`);
if (failures.length > 0) {
  process.exit(1);
}
