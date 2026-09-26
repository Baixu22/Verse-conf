#!/usr/bin/env node
/**
 * 零安装的 VerseConf 工具协议服务端。
 *
 * 这个进程只依赖 Node 与包内的 WebAssembly 模块：宿主不需要 Rust 工具链，
 * 也不需要预编译的本机二进制。它跑的是与本机 `verseconf-mcp` 完全相同的
 * Rust 实现（同一套工具函数），因此两条分发路径的响应逐字节一致。
 *
 * 用法（与本机二进制一致）：
 *   verseconf-mcp-wasm                      以 stdio 逐行 JSON-RPC 方式运行
 *   verseconf-mcp-wasm --list-tools         打印五个工具的描述与输入契约
 *   verseconf-mcp-wasm --call <工具名> [JSON 参数]
 *   verseconf-mcp-wasm --help
 */

import process from 'node:process';
import readline from 'node:readline';

import glue from '../pkg/verseconf_wasm.js';

const USAGE = `verseconf-mcp-wasm - 用 WebAssembly 运行的工具协议服务（需要 Node，不需要 Rust 工具链）

用法：
  verseconf-mcp-wasm                      以 stdio 逐行 JSON-RPC 方式运行（宿主默认接入方式）
  verseconf-mcp-wasm --list-tools         打印五个工具的描述与输入契约
  verseconf-mcp-wasm --call <工具名> [JSON 参数]
                                          单次调用工具，便于冒烟测试与排错
  verseconf-mcp-wasm --help               显示本帮助
`;

const args = process.argv.slice(2);
const pretty = (value) => JSON.stringify(value, null, 2);

function listTools() {
  process.stdout.write(`${pretty(JSON.parse(glue.tools_json()))}\n`);
}

function callOnce(name, argumentsJson) {
  if (!name) {
    process.stderr.write('--call 需要工具名，例如 --call verseconf_validate\n');
    process.exit(2);
  }

  let envelope;
  try {
    envelope = JSON.parse(glue.call_tool_json(name, argumentsJson ?? '{}'));
  } catch (error) {
    process.stderr.write(`参数不是合法 JSON：${error.message}\n`);
    process.exit(2);
  }

  if (envelope.isError) {
    process.stderr.write(`${pretty(envelope.structuredContent)}\n`);
    process.exit(1);
  }

  process.stdout.write(`${pretty(envelope.structuredContent)}\n`);
}

function serveStdio() {
  const server = new glue.WasmMcpServer();
  const lines = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });

  lines.on('line', (line) => {
    const response = server.handle_line(line);
    if (response !== undefined) {
      process.stdout.write(`${response}\n`);
    }
  });
}

switch (args[0]) {
  case '--help':
  case '-h':
    process.stdout.write(USAGE);
    break;
  case '--list-tools':
    listTools();
    break;
  case '--call':
    callOnce(args[1], args[2]);
    break;
  default:
    serveStdio();
    break;
}
