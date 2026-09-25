import { type CancellationToken, type OutputChannel, type TextDocument, Uri, workspace } from 'coc.nvim';
import { spawn } from 'node:child_process';
import namedRegexp from 'named-js-regexp';
import path from 'node:path';
import which from 'which';
import { PythonExecutionService } from '../../processService';
import { LintMessageSeverity, type ILinterInfo, type ILintMessage, type LinterId, Product } from '../../types';
import { BaseLinter } from './baseLinter';

const COLUMN_OFF_SET = 1;

// Allow negative column numbers (https://github.com/PyCQA/pylint/issues/1822)
const FLAKE8_REGEX = '(?<line>\\d+),(?<column>-?\\d+),(?<type>\\w+),(?<code>\\w+\\d+):(?<message>.*)\\r?(\\n|$)';

interface IRegexGroup {
  line: number;
  column: number;
  code: string;
  message: string;
  type: string;
  file?: string;
}

export class Flake8 extends BaseLinter {
  constructor(info: ILinterInfo, outputChannel: OutputChannel) {
    super(info, outputChannel, COLUMN_OFF_SET);
  }

  protected async runLinter(document: TextDocument, cancellation: CancellationToken): Promise<ILintMessage[]> {
    const fsPath = Uri.parse(document.uri).fsPath;
    const args = ['--format=%(row)d,%(col)d,%(code).1s,%(code)s:%(text)s', '--exit-zero'];
    const lintingSettings = this.pythonSettings.linting as any;
    if (lintingSettings['flake8Stdin']) {
      args.push('--stdin-display-name', fsPath, '-');
    } else {
      args.push(fsPath);
    }

    if (!lintingSettings['flake8Enabled']) {
      return [];
    }

    try {
      // Where is the flake8 binary, and with which arguments do we run it?
      const configuredCmd = workspace.expand(lintingSettings['flake8Path']);
      const execPath = which.sync(configuredCmd, { nothrow: true }) || configuredCmd;
      const configArgs = Array.isArray(lintingSettings['flake8Args']) ? lintingSettings['flake8Args'] : [];
      const execArgs = configArgs.concat(args);
      let moduleName: string | undefined;

      // If path information is not available, then treat it as a module,
      if (path.basename(execPath) === execPath) {
        moduleName = execPath;
      }
      const executionInfo = { execPath, moduleName, args: execArgs, product: Product.flake8 };

      this.outputChannel.appendLine(`${'#'.repeat(10)} Run linter flake8:`);
      this.outputChannel.appendLine(JSON.stringify(executionInfo));
      this.outputChannel.appendLine('');

      let result = '';
      if (lintingSettings['flake8Stdin']) {
        // Feed the document through stdin instead of pointing at a file.
        let command = executionInfo.execPath;
        let childArgs = executionInfo.args;
        if (executionInfo.moduleName) {
          command = this.pythonSettings.pythonPath;
          childArgs = ['-m', executionInfo.moduleName, ...executionInfo.args];
        }
        const child = spawn(command, childArgs, { cwd: workspace.root });
        result = await new Promise((resolve) => {
          child.stdin.setDefaultEncoding('utf8');
          child.stdin.write(document.getText());
          child.stdin.end();

          let resultText = '';
          child.stdout.on('data', (data) => {
            resultText += data.toString('utf-8').trim();
          });
          child.on('close', () => {
            resolve(resultText);
          });
        });
      } else {
        const service = new PythonExecutionService();
        result = (
          await service.exec(executionInfo, { cwd: workspace.root, token: cancellation, mergeStdOutErr: false })
        ).stdout;
      }

      this.outputChannel.append(`${'#'.repeat(10)} Linting Output - flake8 ${'#'.repeat(10)}\n`);
      this.outputChannel.append(result);
      this.outputChannel.appendLine('');

      // Decode the flake8 output, line by line, against the format strings above.
      const messages: ILintMessage[] = [];
      const outputLines = result.split(/\r?\n/g);
      for (const line of outputLines) {
        try {
          const compiledRegexp = namedRegexp(FLAKE8_REGEX, 'g');
          const rawMatch = compiledRegexp.exec(line);
          let lineMatch: IRegexGroup | undefined;
          if (rawMatch) {
            // @ts-expect-error
            lineMatch = rawMatch.groups() as IRegexGroup;
          }
          if (!lineMatch) {
            continue;
          }

          lineMatch.line = Number(lineMatch.line as any);
          lineMatch.column = Number(lineMatch.column as any);

          messages.push({
            code: lineMatch.code,
            message: lineMatch.message,
            column: Number.isNaN(lineMatch.column) || lineMatch.column <= 0 ? 0 : lineMatch.column - this.columnOffset,
            line: lineMatch.line,
            type: lineMatch.type,
            provider: 'flake8' as LinterId,
            file: lineMatch.file,
          });
          if (messages.length >= this.pythonSettings.linting.maxNumberOfProblems) {
            break;
          }
        } catch (err) {
          this.outputChannel.appendLine(`${'#'.repeat(10)} Linter flake8 failed to parse the line:`);
          this.outputChannel.appendLine(line);
          if (typeof err === 'string') {
            this.outputChannel.appendLine(err);
          } else if (err instanceof Error) {
            this.outputChannel.appendLine(err.message);
          }
        }
      }

      // Name the severity that flake8 wants for this type of message.
      const severityFor = (errorType: string): LintMessageSeverity => {
        const categorySeverity = (this.pythonSettings.linting as any).flake8CategorySeverity;
        if (categorySeverity[errorType]) {
          const severityName = categorySeverity[errorType];
          switch (severityName) {
            case 'Error':
              return LintMessageSeverity.Error;
            case 'Hint':
              return LintMessageSeverity.Hint;
            case 'Information':
              return LintMessageSeverity.Information;
            case 'Warning':
              return LintMessageSeverity.Warning;
            default: {
              if (LintMessageSeverity[severityName]) {
                return LintMessageSeverity[severityName] as any as LintMessageSeverity;
              }
            }
          }
        }
        return LintMessageSeverity.Information;
      };
      for (const msg of messages) {
        msg.severity = severityFor(msg.type);
      }
      return messages;
    } catch (error) {
      this.outputChannel.appendLine('Linting with flake8 failed:');
      if (error instanceof Error) {
        this.outputChannel.appendLine(error.message.toString());
      }
      return [];
    }
  }
}
