export interface DigestLineWriter {
  writeLine(line: string): void;
}

export class StringDigestLineWriter implements DigestLineWriter {
  private readonly _lines: string[];

  constructor() {
    this._lines = [];
  }

  public writeLine(line: string): void {
    this._lines.push(line);
  }

  public render(): string {
    return this._lines.join('\n');
  }
}
