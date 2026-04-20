export class VerseConf {
  constructor(source: string);

  get_string(path: string): string | undefined;
  get_number(path: string): number | undefined;
  get_boolean(path: string): boolean | undefined;
  has_key(path: string): boolean;
  to_json(): string;
  validate(): boolean;
}

export function parse_config(source: string): VerseConf;
export function version(): string;
