#!/usr/bin/env node
/**
 * 构建 npm 包产物。
 *
 *   1. wasm-pack --target nodejs -> pkg/      宿主用 require/import 加载（Node、Electron、VS Code 扩展宿主）
 *   2. wasm-pack --target web    -> pkg-web/  浏览器与打包器加载（<script type="module"> + init()）
 *   3. tsc（ESM）                -> dist/index.mjs
 *   4. tsc（CJS）                -> dist/index.cjs
 *
 * 两次 tsc 都产出 `dist/index.js`，靠重命名区分模块格式。源码只有一个入口、
 * 没有跨文件相对导入，因此重命名不会破坏模块解析。
 *
 * 另外会删除 wasm-pack 在输出目录里写的 `.gitignore`：npm 在没有 `.npmignore`
 * 的子目录里会回退到 `.gitignore`，而 wasm-pack 生成的 `.gitignore` 内容是 `*`，
 * 结果就是「package.json 的 files 里写了 pkg，发布包里却没有 pkg」。
 */

import { execFileSync } from 'node:child_process';
import { existsSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const packageRoot = path.resolve(here, '..');
const crateRoot = path.resolve(packageRoot, '..');

function run(command, args, cwd, { shell = false } = {}) {
  execFileSync(command, args, { cwd, stdio: 'inherit', shell });
}

function wasmPack(target, outDir) {
  const args = ['build', '--target', target, '--out-dir', outDir];
  if (process.platform === 'win32') {
    // wasm-pack 在 Windows 上是 .cmd，只能交给 cmd.exe；这里不用 shell 选项，
    // 避免参数被拼接而不是转义。
    run('cmd.exe', ['/c', 'wasm-pack', ...args], crateRoot);
  } else {
    run('wasm-pack', args, crateRoot);
  }
}

function tsc(project) {
  const tscBin = path.join(packageRoot, 'node_modules', 'typescript', 'bin', 'tsc');
  if (!existsSync(tscBin)) {
    throw new Error('找不到本地 typescript，请先运行 npm install');
  }
  // 直接用 node 跑 tsc：node 的绝对路径含空格，绝不能经过 shell。
  run(process.execPath, [tscBin, '-p', project], packageRoot);
}

function renameOutput(from, to) {
  const source = path.join(packageRoot, 'dist', from);
  const target = path.join(packageRoot, 'dist', to);
  if (!existsSync(source)) {
    throw new Error(`构建产物缺失：dist/${from}`);
  }
  rmSync(target, { force: true });
  renameSync(source, target);
}

/** 让 npm 打包时不会因为 wasm-pack 的 `.gitignore` 丢掉运行时文件 */
function dropWasmPackGitignore(...dirs) {
  for (const dir of dirs) {
    const target = path.join(packageRoot, dir);
    rmSync(path.join(target, '.gitignore'), { force: true });
    // 放一个空的 `.npmignore`：npm 在同一目录里优先用它，于是不会回退到
    // `.gitignore`，即使之后有人重新跑了一次 wasm-pack 也不会再丢文件。
    writeFileSync(path.join(target, '.npmignore'), '');
  }
}

console.log('==> wasm-pack (nodejs)');
wasmPack('nodejs', 'js-api/pkg');

console.log('==> wasm-pack (web)');
wasmPack('web', 'js-api/pkg-web');

console.log('==> tsc (ESM -> dist/index.mjs)');
tsc('tsconfig.json');
renameOutput('index.js', 'index.mjs');

console.log('==> tsc (CJS -> dist/index.cjs)');
tsc('tsconfig.cjs.json');
renameOutput('index.js', 'index.cjs');

dropWasmPackGitignore('pkg', 'pkg-web');

console.log('==> 构建完成：dist/index.mjs、dist/index.cjs、dist/index.d.ts、pkg/、pkg-web/');
