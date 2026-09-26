/**
 * VerseConf JavaScript 包。
 *
 * 这个包不重新实现任何能力：它加载的是与本机二进制 `verseconf-mcp`
 * 完全相同的 Rust 实现编译出的 WebAssembly 模块，因此同一份输入在
 * 「本机原生二进制」与「wasm 模块」上返回完全相同的 JSON。
 *
 * 宿主只需要一个 JS 运行时（Agent 宿主本来就带），不需要 Rust 工具链，
 * 也不需要安装任何本机二进制。注意：本包**未发布到 npm**，用之前要先从源码构建。
 *
 * ```js
 * const { validate, audit } = require('verseconf');
 * validate('port = 8080\n');            // -> { isError: false, structuredContent: { valid: true, ... } }
 * ```
 */

import glue from '../pkg/verseconf_wasm.js';

/** 校验配置：解析 + 结构/schema 校验 */
export const TOOL_VALIDATE = 'verseconf_validate';
/** 安全审计：敏感数据与不安全配置检查 */
export const TOOL_AUDIT = 'verseconf_audit';
/** 意图应用：按编辑计划做确定性最小改动 */
export const TOOL_APPLY_EDIT = 'verseconf_apply_edit';
/** 区间编辑：对指定字节区间做替换，走同一套写入前校验 */
export const TOOL_EDIT_RANGE = 'verseconf_edit_range';
export const TOOL_CHECK_WRITE = 'verseconf_check_write';

/** 工具返回的文本内容块 */
export interface ToolContent {
  type: 'text';
  text: string;
}

/** 工具失败时的结构化原因 */
export interface ToolError {
  code: string;
  message: string;
  details: unknown;
}

/** `tools/call` 的结果信封，与原生 stdio 服务端逐字节一致 */
export interface ToolResult<T> {
  content: ToolContent[];
  structuredContent: T | ToolError;
  isError: boolean;
}

/** 宿主可发现的一个工具 */
export interface ToolDescriptor {
  name: string;
  description: string;
  inputSchema: Record<string, unknown>;
}

/** 服务端自述信息 */
export interface ServerInfo {
  name: string;
  version: string;
  protocolVersion: string;
}

export interface Diagnostic {
  code: string;
  message: string;
  line: number;
  column: number;
}

export interface ValidateResult {
  valid: boolean;
  diagnostics: Diagnostic[];
}

export interface AuditFinding {
  rule_id: string;
  severity: string;
  category: string;
  title: string;
  description: string;
  location: string;
  recommendation: string;
}

export interface AuditSummary {
  total: number;
  critical: number;
  high: number;
  medium: number;
  low: number;
  info: number;
}

export interface AuditResult {
  findings: AuditFinding[];
  summary: AuditSummary;
}

export interface AppliedEdit {
  op: string;
  path: string;
  before: string | null;
  after: string | null;
  reason: string | null;
}

export interface ApplyEditResult {
  source: string;
  applied: AppliedEdit[];
}

export interface ReplacedRange {
  start: number;
  end: number;
  before: string;
  after: string;
}

export interface EditRangeResult {
  source: string;
  replaced: ReplacedRange;
}

export interface CheckWriteResult {
  allowed: true;
  baseline_bytes: number;
  candidate_bytes: number;
}

/**
 * 编辑计划（意图契约）。完整定义见
 * `crates/verseconf-core/schemas/edit-plan.schema.json`。
 *
 * 列表按名称而不是下标定位，且只替换目标值的字节区间。
 */
export interface EditPlan {
  version: string;
  edits: Array<Record<string, unknown>>;
}

function callTool<T>(name: string, args: unknown): ToolResult<T> {
  return JSON.parse(glue.call_tool_json(name, JSON.stringify(args ?? {}))) as ToolResult<T>;
}

/** 校验配置文本，返回与原生命令行/工具协议一致的结构化诊断 */
export function validate(source: string, options: { strict?: boolean } = {}): ToolResult<ValidateResult> {
  return callTool<ValidateResult>(TOOL_VALIDATE, {
    source,
    strict: options.strict === true,
  });
}

/** 安全审计配置文本 */
export function audit(source: string): ToolResult<AuditResult> {
  return callTool<AuditResult>(TOOL_AUDIT, { source });
}

/** 按编辑意图契约做确定性最小改动（不写文件，只返回改动后的文本） */
export function applyEdit(source: string, plan: EditPlan | string): ToolResult<ApplyEditResult> {
  return callTool<ApplyEditResult>(TOOL_APPLY_EDIT, { source, plan });
}

/** 区间编辑原语：把 [start, end) 替换为 replacement */
export function editRange(
  source: string,
  start: number,
  end: number,
  replacement: string
): ToolResult<EditRangeResult> {
  return callTool<EditRangeResult>(TOOL_EDIT_RANGE, { source, start, end, replacement });
}

/** 写前检查：给定原文与候选文本，判断这次改动是否允许落盘（不产生改动） */
export function checkWrite(
  baseline: string,
  candidate: string,
  options: { schema?: string } = {}
): ToolResult<CheckWriteResult> {
  return callTool<CheckWriteResult>(TOOL_CHECK_WRITE, {
    baseline,
    candidate,
    ...(options.schema === undefined ? {} : { schema: options.schema }),
  });
}

/** `tools/list` 的五个工具描述 */
export function tools(): ToolDescriptor[] {
  return (JSON.parse(glue.tools_json()) as { tools: ToolDescriptor[] }).tools;
}

/** 服务端名称、版本与协议版本 */
export function serverInfo(): ServerInfo {
  return JSON.parse(glue.server_info_json()) as ServerInfo;
}

/** wasm 模块自身的版本号 */
export function getVersion(): string {
  return glue.version();
}

/**
 * 逐行 JSON-RPC 会话，行为与原生 `verseconf-mcp` 的 stdio 会话一致。
 *
 * 宿主可以用它把 MCP 协议桥到任何自己的传输层上。
 */
export class McpSession {
  private server: InstanceType<typeof glue.WasmMcpServer>;

  constructor() {
    this.server = new glue.WasmMcpServer();
  }

  /** 处理一行输入；通知（无 `id`）与空行返回 undefined */
  handleLine(line: string): string | undefined {
    return this.server.handle_line(line);
  }

  /** 宿主是否已经完成 `initialize` 握手 */
  isInitialized(): boolean {
    return this.server.is_initialized();
  }
}

/** 值访问：解析一次，然后按路径取值 */
export class VerseConf {
  private wasm: InstanceType<typeof glue.VerseConf>;

  constructor(source: string) {
    this.wasm = new glue.VerseConf(source);
  }

  getString(path: string): string | undefined {
    return this.wasm.get_string(path);
  }

  getNumber(path: string): number | undefined {
    return this.wasm.get_number(path);
  }

  getBoolean(path: string): boolean | undefined {
    return this.wasm.get_boolean(path);
  }

  hasKey(path: string): boolean {
    return this.wasm.has_key(path);
  }

  toJson(): string {
    return this.wasm.to_json();
  }

  validate(): boolean {
    return this.wasm.validate();
  }
}

/** 解析配置文本，解析失败时抛出 */
export function parseConfig(source: string): VerseConf {
  return new VerseConf(source);
}

/** 解析配置文本并直接返回 JSON 对象 */
export function parseJson(source: string): unknown {
  return JSON.parse(glue.parse_config(source) as string);
}

// 兼容旧版入口：原样再导出 wasm 层符号。
export const VerseConfWasm = glue.VerseConf;
export const wasmParse = glue.parse_config as (source: string) => string;
export const wasmVersion = glue.version;
