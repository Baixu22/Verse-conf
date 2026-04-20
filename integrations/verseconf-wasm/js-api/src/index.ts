import { VerseConf as WasmVerseConf, parse_config, version } from '../pkg/verseconf_wasm';

export class VerseConf {
  private wasm: WasmVerseConf;

  constructor(source: string) {
    this.wasm = new WasmVerseConf(source);
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

export function parseConfig(source: string): VerseConf {
  parse_config(source);
  return new VerseConf(source);
}

export function getVersion(): string {
  return version();
}

export { VerseConf as VerseConfWasm, parse_config as wasmParse, version };

export type { VerseConf as VerseConfType } from '../pkg/verseconf_wasm';