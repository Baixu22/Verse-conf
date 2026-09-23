#!/usr/bin/env node
/**
 * 扩展包验收：`vsce ls` 的清单与真正打出来的 .vsix 内容都必须包含运行所需文件。
 *
 * 这条检查针对的是「文档要求安装的扩展包仓库里并不存在」这个缺陷：
 * 只跑 `tsc` 通过不代表扩展能被安装，必须真的产出一个 .vsix 并核对它的内容。
 *
 * 用法：
 *   node scripts/verify-package.mjs                 # 校验结构
 *   node scripts/verify-package.mjs --require-server # 额外要求包内带当前平台的语言服务器
 */

import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, readdirSync, rmSync } from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const here = path.dirname(fileURLToPath(import.meta.url));
const extensionRoot = path.resolve(here, '..');
const requireServer = process.argv.includes('--require-server');

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

function vsceBin() {
  const packageJsonPath = require.resolve('@vscode/vsce/package.json', { paths: [extensionRoot] });
  const manifest = JSON.parse(readFileSync(packageJsonPath, 'utf8'));
  const relative = typeof manifest.bin === 'string' ? manifest.bin : manifest.bin.vsce;
  return path.join(path.dirname(packageJsonPath), relative);
}

function runVsce(args) {
  return execFileSync(process.execPath, [vsceBin(), ...args], {
    cwd: extensionRoot,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
  });
}

/** 读取 zip（.vsix 就是 zip）中央目录里的条目名，不依赖任何第三方库 */
function listZipEntries(file) {
  const buffer = readFileSync(file);
  let eocd = -1;
  for (let index = buffer.length - 22; index >= 0; index -= 1) {
    if (buffer.readUInt32LE(index) === 0x06054b50) {
      eocd = index;
      break;
    }
  }
  if (eocd < 0) {
    throw new Error('不是合法的 zip/vsix：找不到 EOCD 记录');
  }

  const total = buffer.readUInt16LE(eocd + 10);
  let offset = buffer.readUInt32LE(eocd + 16);
  const names = [];
  for (let index = 0; index < total; index += 1) {
    if (buffer.readUInt32LE(offset) !== 0x02014b50) {
      throw new Error('中央目录损坏');
    }
    const nameLength = buffer.readUInt16LE(offset + 28);
    const extraLength = buffer.readUInt16LE(offset + 30);
    const commentLength = buffer.readUInt16LE(offset + 32);
    names.push(buffer.toString('utf8', offset + 46, offset + 46 + nameLength));
    offset += 46 + nameLength + extraLength + commentLength;
  }
  return names;
}

/** 磁盘上实际存在的语言服务器二进制（相对扩展根目录） */
function serverBinariesOnDisk() {
  const binRoot = path.join(extensionRoot, 'server', 'bin');
  if (!existsSync(binRoot)) {
    return [];
  }
  const found = [];
  for (const entry of readdirSync(binRoot, { withFileTypes: true })) {
    const absolute = path.join(binRoot, entry.name);
    if (entry.isDirectory()) {
      for (const inner of readdirSync(absolute)) {
        if (inner.startsWith('verseconf-lsp')) {
          found.push(path.posix.join('server/bin', entry.name, inner));
        }
      }
    } else if (entry.name.startsWith('verseconf-lsp')) {
      found.push(path.posix.join('server/bin', entry.name));
    }
  }
  return found.sort();
}

const REQUIRED = [
  'package.json',
  'README.md',
  'CHANGELOG.md',
  'LICENSE',
  'language-configuration.json',
  'syntaxes/verseconf.tmLanguage.json',
  'out/extension.js',
  'out/serverPath.js',
];

/**
 * vsce 会把 readme / changelog / license 改名成小写（LICENSE 还会变成 LICENSE.txt），
 * 所以核对 .vsix 时接受这几种写法。
 */
const VSIX_ALIASES = {
  'README.md': ['README.md', 'readme.md'],
  'CHANGELOG.md': ['CHANGELOG.md', 'changelog.md'],
  'LICENSE': ['LICENSE', 'LICENSE.txt', 'license.txt'],
};

console.log('== vsce ls');
const listed = runVsce(['ls'])
  .split('\n')
  .map((line) => line.trim())
  .filter((line) => line.length > 0);

for (const relative of REQUIRED) {
  check(`清单包含 ${relative}`, () => {
    if (!listed.includes(relative)) {
      throw new Error(`vsce ls 里没有 ${relative}`);
    }
  });
}

const binaries = serverBinariesOnDisk();
console.log(`== 磁盘上的语言服务器：${binaries.length === 0 ? '（无）' : binaries.join('、')}`);
if (requireServer) {
  check('至少有一个平台的语言服务器可打包', () => {
    if (binaries.length === 0) {
      throw new Error('server/bin 下没有任何 verseconf-lsp 二进制');
    }
  });
}
for (const binary of binaries) {
  check(`清单包含 ${binary}`, () => {
    if (!listed.includes(binary)) {
      throw new Error(`vsce ls 里没有 ${binary}`);
    }
  });
}

console.log('== vsce package');
const vsixPath = path.join(tmpdir(), `verseconf-extension-${process.pid}.vsix`);
try {
  const output = runVsce(['package', '--out', vsixPath]);
  const lastLine = output.trim().split('\n').pop() ?? '';
  console.log(`  ${lastLine}`);

  check('.vsix 已产出', () => {
    if (!existsSync(vsixPath)) {
      throw new Error(`没有生成 ${vsixPath}`);
    }
  });

  const entries = listZipEntries(vsixPath);
  for (const relative of REQUIRED) {
    const accepted = (VSIX_ALIASES[relative] ?? [relative]).map((name) => `extension/${name}`);
    check(`.vsix 包含 extension/${relative}`, () => {
      if (!accepted.some((name) => entries.includes(name))) {
        throw new Error(`.vsix 里没有 ${accepted.join(' 或 ')}`);
      }
    });
  }
  check('.vsix 不含 sourcemap（.vscodeignore 里的 **/*.map 生效）', () => {
    const maps = entries.filter((entry) => entry.endsWith('.map'));
    if (maps.length > 0) {
      throw new Error(`.vsix 里仍有 sourcemap：${maps.join('、')}`);
    }
  });
  for (const binary of binaries) {
    check(`.vsix 包含 extension/${binary}`, () => {
      if (!entries.includes(`extension/${binary}`)) {
        throw new Error(`.vsix 里没有 extension/${binary}`);
      }
    });
  }
  check('.vsix 包含扩展清单', () => {
    for (const required of ['extension.vsixmanifest', '[Content_Types].xml']) {
      if (!entries.includes(required)) {
        throw new Error(`.vsix 里没有 ${required}`);
      }
    }
  });
  check('.vsix 带上了语言客户端运行时依赖', () => {
    if (!entries.includes('extension/node_modules/vscode-languageclient/package.json')) {
      throw new Error('.vsix 里没有 vscode-languageclient');
    }
  });
} finally {
  rmSync(vsixPath, { force: true });
}

console.log(`\n${passed} 项通过，${failures.length} 项失败`);
if (failures.length > 0) {
  process.exit(1);
}
