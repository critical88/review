// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

import {
  CancellationTokenSource,
  Diagnostic,
  type DiagnosticCollection,
  DiagnosticSeverity,
  type DocumentFilter,
  languages,
  type OutputChannel,
  Position,
  Range,
  type TextDocument,
  Uri,
  window,
  workspace,
} from 'coc.nvim';
import fs from 'node:fs';
import { Minimatch } from 'minimatch';
import path from 'node:path';
import { PythonSettings } from '../../configSettings';
import {
  LintMessageSeverity,
  type ILinter,
  Product,
  type ILintMessage,
  type ILinterInfo,
  LinterErrors,
} from '../../types';
import { Bandit } from './bandit';
import { Flake8 } from './flake8';
import { LinterInfo } from './linterInfo';
import { MyPy } from './mypy';
import { Prospector } from './prospector';
import { PyCodeStyle } from './pycodestyle';
import { PyDocStyle } from './pydocstyle';
import { Pyflakes } from './pyflakes';
import { Pylama } from './pylama';
import { Pylint } from './pylint';
import { Pytype } from './pytype';
import { Ruff } from './ruff';

const PYTHON: DocumentFilter = { language: 'python' };

const lintSeverityToVSSeverity = new Map<LintMessageSeverity, DiagnosticSeverity>();
lintSeverityToVSSeverity.set(LintMessageSeverity.Error, DiagnosticSeverity.Error);
lintSeverityToVSSeverity.set(LintMessageSeverity.Hint, DiagnosticSeverity.Hint);
lintSeverityToVSSeverity.set(LintMessageSeverity.Information, DiagnosticSeverity.Information);
lintSeverityToVSSeverity.set(LintMessageSeverity.Warning, DiagnosticSeverity.Warning);

class DisabledLinter implements ILinter {
  constructor(private configService: PythonSettings) {}
  public get info() {
    return new LinterInfo(Product.pylint, 'pylint', this.configService);
  }
  public async lint(): Promise<ILintMessage[]> {
    return [];
  }
}

export class LintingEngine {
  private diagnosticCollection: DiagnosticCollection;
  private pendingLintings = new Map<string, CancellationTokenSource>();
  private configService: PythonSettings;
  private outputChannel: OutputChannel;
  protected linters: ILinterInfo[];

  constructor() {
    this.outputChannel = window.createOutputChannel('coc-pyright-linting');
    this.diagnosticCollection = languages.createDiagnosticCollection('python');
    this.configService = PythonSettings.getInstance();
    this.linters = [
      new LinterInfo(Product.bandit, 'bandit', this.configService),
      new LinterInfo(Product.flake8, 'flake8', this.configService),
      new LinterInfo(Product.pylint, 'pylint', this.configService, ['.pylintrc', 'pylintrc']),
      new LinterInfo(Product.mypy, 'mypy', this.configService),
      new LinterInfo(Product.pycodestyle, 'pycodestyle', this.configService),
      new LinterInfo(Product.prospector, 'prospector', this.configService),
      new LinterInfo(Product.pydocstyle, 'pydocstyle', this.configService),
      new LinterInfo(Product.pyflakes, 'pyflakes', this.configService),
      new LinterInfo(Product.pylama, 'pylama', this.configService),
      new LinterInfo(Product.pytype, 'pytype', this.configService),
      new LinterInfo(Product.ruff, 'ruff', this.configService),
    ];
  }

  public get diagnostics(): DiagnosticCollection {
    return this.diagnosticCollection;
  }

  public clearDiagnostics(document: TextDocument): void {
    if (this.diagnosticCollection.has(document.uri)) {
      this.diagnosticCollection.delete(document.uri);
    }
  }

  public async lintOpenPythonFiles(): Promise<DiagnosticCollection> {
    this.diagnosticCollection.clear();
    const promises = workspace.textDocuments.map(async (document) => this.lintDocument(document));
    await Promise.all(promises);
    return this.diagnosticCollection;
  }

  public async lintDocument(document: TextDocument, onChange = false): Promise<void> {
    this.diagnosticCollection.set(document.uri, []);

    // Check if we need to lint this document
    const initialSettings = this.configService;
    if (!initialSettings.linting.enabled) {
      this.outputChannel.appendLine(`${'#'.repeat(5)} linting is disabled by python.linting.enabled`);
      return;
    }

    if (document.languageId !== PYTHON.language) {
      return;
    }

    const fsPath = Uri.parse(document.uri).fsPath;
    if (initialSettings.stdLibs.some((p) => fsPath.startsWith(p))) {
      return;
    }

    const relativeFileName = path.relative(workspace.root, fsPath);
    // { dot: true } is important so dirs like `.venv` will be matched by globs
    const ignoreMinmatches = initialSettings.linting.ignorePatterns.map(
      (pattern) => new Minimatch(pattern, { dot: true }),
    );
    if (ignoreMinmatches.some((matcher) => matcher.match(fsPath) || matcher.match(relativeFileName))) {
      this.outputChannel.appendLine(`${'#'.repeat(5)} linting is ignored by python.linting.ignorePatterns`);
      return;
    }

    const fileExists = fs.existsSync(fsPath);
    if (!fileExists) {
      this.outputChannel.appendLine(`${'#'.repeat(5)} linting is disabled because file is not exists: ${fsPath}`);
      return;
    }

    if (this.pendingLintings.has(fsPath)) {
      this.pendingLintings.get(fsPath)!.cancel();
      this.pendingLintings.delete(fsPath);
    }

    const cancelToken = new CancellationTokenSource();
    cancelToken.token.onCancellationRequested(() => {
      if (this.pendingLintings.has(fsPath)) {
        this.pendingLintings.delete(fsPath);
      }
    });

    this.pendingLintings.set(fsPath, cancelToken);

    const activeLinters = this.getActiveLinters().filter((l) => (onChange ? l.stdinSupport : true));
    const promises: Promise<ILintMessage[]>[] = activeLinters.map(async (info: ILinterInfo) => {
      this.outputChannel.appendLine(`Using python from ${this.configService.pythonPath}\n`);
      this.outputChannel.appendLine(`${'#'.repeat(10)} active linter: ${info.id}`);
      // Build the linter right here instead of asking the factory for it.
      let linter: ILinter;
      if (!this.configService.linting.enabled) {
        linter = new DisabledLinter(this.configService);
      } else {
        switch (info.product) {
          case Product.bandit:
            linter = new Bandit(info, this.outputChannel);
            break;
          case Product.flake8:
            linter = new Flake8(info, this.outputChannel);
            break;
          case Product.pylint:
            linter = new Pylint(info, this.outputChannel);
            break;
          case Product.mypy:
            linter = new MyPy(info, this.outputChannel);
            break;
          case Product.prospector:
            linter = new Prospector(info, this.outputChannel);
            break;
          case Product.pylama:
            linter = new Pylama(info, this.outputChannel);
            break;
          case Product.pydocstyle:
            linter = new PyDocStyle(info, this.outputChannel);
            break;
          case Product.pycodestyle:
            linter = new PyCodeStyle(info, this.outputChannel);
            break;
          case Product.pytype:
            linter = new Pytype(info, this.outputChannel);
            break;
          case Product.pyflakes:
            linter = new Pyflakes(info, this.outputChannel);
            break;
          case Product.ruff:
            linter = new Ruff(info, this.outputChannel);
            break;
          default:
            throw new Error('Linter manager: Unknown linter');
        }
      }
      const promise = linter.lint(document, cancelToken.token);
      return promise;
    });

    // linters will resolve asynchronously - keep a track of all
    // diagnostics reported as them come in.
    let diagnostics: Diagnostic[] = [];
    const settings = this.configService;

    for (const p of promises) {
      const msgs = await p;
      if (cancelToken.token.isCancellationRequested) {
        break;
      }

      const doc = workspace.getDocument(document.uri);
      if (doc) {
        // Build the message and suffix the message with the name of the linter used.
        for (const m of msgs) {
          if (
            doc
              .getline(m.line - 1)
              .trim()
              .startsWith('%') &&
            (m.code === LinterErrors.pylint.InvalidSyntax ||
              m.code === LinterErrors.prospector.InvalidSyntax ||
              m.code === LinterErrors.flake8.InvalidSyntax)
          ) {
            continue;
          }
          let start = Position.create(m.line > 0 ? m.line - 1 : 0, m.column);
          const endLine = m.endLine ?? m.line;
          const endColumn = m.endColumn ?? m.column + 1;
          let end = Position.create(endLine > 0 ? endLine - 1 : 0, endColumn);

          const ms = /['"](.*?)['"]/g.exec(m.message);
          if (ms && ms.length > 0) {
            const line = workspace.getDocument(document.uri)?.getline(m.line - 1);
            if (line?.includes(ms[1])) {
              const s = m.column > line.indexOf(ms[1]) ? m.column : line.indexOf(ms[1]);
              start = Position.create(m.line - 1, s);
              end = Position.create(m.line - 1, s + ms[1].length);
            }
          }

          const range = Range.create(start, end);
          const severity = lintSeverityToVSSeverity.get(m.severity!)!;
          const diagnostic = Diagnostic.create(range, m.message, severity);
          diagnostic.code = m.code;
          if (m.url) {
            diagnostic.codeDescription = { href: m.url };
          }
          diagnostic.source = m.provider;
          // @ts-expect-error
          diagnostic.fix = m.fix;
          diagnostic.tags = m.tags;
          diagnostics.push(diagnostic);
        }
        // Limit the number of messages to the max value.
        diagnostics = diagnostics.filter((_value, index) => index <= settings.linting.maxNumberOfProblems);
      }
    }
    // Set all diagnostics found in this pass, as this method always clears existing diagnostics.
    this.diagnosticCollection.set(document.uri, diagnostics);
  }

  public getAllLinterInfos(): ILinterInfo[] {
    return this.linters;
  }

  public getLinterInfo(product: Product): ILinterInfo {
    const x = this.linters.findIndex((value) => value.product === product);
    if (x >= 0) {
      return this.linters[x];
    }
    throw new Error(`Invalid linter '${Product[product]}'`);
  }

  public getActiveLinters(resource?: Uri): ILinterInfo[] {
    return this.linters.filter((x) => x.isEnabled(resource));
  }
}
