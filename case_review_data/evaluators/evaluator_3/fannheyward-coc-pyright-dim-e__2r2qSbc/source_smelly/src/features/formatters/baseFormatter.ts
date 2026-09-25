import {
  type CancellationToken,
  type FormattingOptions,
  type OutputChannel,
  type Position,
  Range,
  type TextDocument,
  TextEdit,
  type Thenable,
  Uri,
  window,
  workspace,
} from 'coc.nvim';
import fs from 'node:fs';
import md5 from 'md5';
import { EOL } from 'node:os';
import path from 'node:path';
import which from 'which';
import { isNotInstalledError, PythonExecutionService } from '../../processService';
import type { FormatterId, IPythonSettings } from '../../types';

function getTempFileWithDocumentContents(document: TextDocument): Promise<string> {
  return new Promise<string>((resolve, reject) => {
    const fsPath = Uri.parse(document.uri).fsPath;
    const ext = path.extname(fsPath);
    // Don't create file in temp folder since external utilities
    // look into configuration files in the workspace and are not able
    // to find custom rules if file is saved in a random disk location.
    // This means temp file has to be created in the same folder
    // as the original one and then removed.

    const fileName = `${fsPath}.${md5(document.uri)}${ext}`;
    fs.writeFile(fileName, document.getText(), (ex) => {
      if (ex) {
        reject(new Error(`Failed to create a temporary file, ${ex.message}`));
      }
      resolve(fileName);
    });
  });
}

export abstract class BaseFormatter {
  constructor(
    public readonly Id: FormatterId,
    public readonly pythonSettings: IPythonSettings,
    public readonly outputChannel: OutputChannel,
  ) {}

  public abstract formatDocument(
    document: TextDocument,
    options: FormattingOptions,
    token: CancellationToken,
    range?: Range,
  ): Thenable<TextEdit[]>;
  protected getDocumentPath(document: TextDocument, fallbackPath?: string): string {
    const filepath = Uri.parse(document.uri).fsPath;
    if (fallbackPath && path.basename(filepath) === filepath) {
      return fallbackPath;
    }
    return path.dirname(filepath);
  }
  protected getWorkspaceUri(document: TextDocument): Uri | undefined {
    const filepath = Uri.parse(document.uri).fsPath;
    if (!filepath.startsWith(workspace.root)) return;
    return Uri.file(workspace.root);
  }

  protected async provideDocumentFormattingEdits(
    document: TextDocument,
    _options: FormattingOptions,
    token: CancellationToken,
    args: string[],
    root?: string,
  ): Promise<TextEdit[]> {
    if (this.pythonSettings.stdLibs.some((p) => Uri.parse(document.uri).fsPath.startsWith(p))) {
      return [];
    }
    // autopep8 and yapf have the ability to read from the process input stream and return the formatted code out of the output stream.
    // However they don't support returning the diff of the formatted text when reading data from the input stream.
    // Yet getting text formatted that way avoids having to create a temporary file, however the diffing will have
    // to be done here in node (extension), i.e. extension CPU, i.e. less responsive solution.
    const filepath = Uri.parse(document.uri).fsPath;
    const tempFile = await this.createTempFile(document);
    if (token?.isCancellationRequested) {
      this.outputChannel.appendLine(`${'#'.repeat(10)} ${this.Id} formatting action is canceled on start`);
      if (filepath !== tempFile) {
        fs.promises.unlink(tempFile).catch(() => {});
      }
      return [];
    }
    args.push(tempFile);

    // Resolve the formatter executable. If it is a bare name rather than a path,
    // it will be run through the python interpreter as a module instead.
    let moduleName: string | undefined;
    let execPath = this.pythonSettings.formatting[`${this.Id}Path`] as string;
    execPath = which.sync(execPath, { nothrow: true }) || execPath;
    if (path.basename(execPath) === execPath) {
      moduleName = execPath;
    }

    const executionInfo = { execPath, moduleName, args };
    this.outputChannel.appendLine(`execPath:   ${executionInfo.execPath}`);
    this.outputChannel.appendLine(`moduleName: ${executionInfo.moduleName}`);
    this.outputChannel.appendLine(`args:       ${executionInfo.args}`);

    const cwd = root?.length ? root : Uri.file(workspace.root).fsPath;
    const pythonToolsExecutionService = new PythonExecutionService();
    try {
      const output = await pythonToolsExecutionService.exec(executionInfo, { cwd, throwOnStdErr: false, token });
      if (output.stderr) {
        throw new Error(output.stderr);
      }
      const data = output.stdout;

      this.outputChannel.appendLine('');
      this.outputChannel.appendLine(`${'#'.repeat(10)} ${this.Id} output:`);
      this.outputChannel.appendLine(data);
      if (token?.isCancellationRequested) {
        this.outputChannel.appendLine(`${'#'.repeat(10)} ${this.Id} formatting action is canceled on success`);
        if (filepath !== tempFile) {
          fs.promises.unlink(tempFile).catch(() => {});
        }
        return [] as TextEdit[];
      }

      // Turn the formatter's unified diff into workspace edits, in place:
      // read the hunk headers, then replay every +/-/space line against the
      // original document text.
      const documentContents = document.getText();
      const textEdits: TextEdit[] = [];
      let patchBody = data;
      if (patchBody.startsWith('---')) {
        // Strip the first two lines; the patch proper starts at the first hunk.
        patchBody = patchBody.substring(patchBody.indexOf('@@'));
      }
      if (patchBody.length > 0) {
        // Remove the text added by unified_diff
        // # Work around missing newline (http://bugs.python.org/issue2142).
        patchBody = patchBody.replace(/\\ No newline at end of file[\r\n]/, '');
        const dmp = require('diff-match-patch') as typeof import('diff-match-patch');
        const patchLines = patchBody.split(/[\r\n]/);
        const blocks: { originLine: number; ops: [number, string][] }[] = [];
        let cursor = 0;
        while (cursor < patchLines.length) {
          const header = patchLines[cursor].match(/^@@ -(\d+),?(\d*) \+(\d+),?(\d*) @@$/);
          if (!header) {
            throw new Error(`Invalid patch string: ${patchLines[cursor]}`);
          }
          const block = { originLine: 0, ops: [] as [number, string][] };
          blocks.push(block);
          block.originLine = parseInt(header[1], 10);
          if (header[2] === '' || header[2] !== '0') {
            block.originLine -= 1;
          }
          cursor += 1;
          while (cursor < patchLines.length) {
            const sign = patchLines[cursor].charAt(0);
            let bodyLine: string;
            try {
              bodyLine = patchLines[cursor].substring(1);
            } catch (_ex) {
              throw new Error('Illegal escape in patch_fromText');
            }
            if (sign === '-') {
              block.ops.push([dmp.DIFF_DELETE, bodyLine]);
            } else if (sign === '+') {
              block.ops.push([dmp.DIFF_INSERT, bodyLine]);
            } else if (sign === ' ') {
              block.ops.push([dmp.DIFF_EQUAL, bodyLine]);
            } else if (sign === '@') {
              break;
            } else if (sign !== '') {
              throw new Error(`Invalid patch mode '${sign}' in: ${bodyLine}`);
            }
            cursor += 1;
          }
        }
        if (blocks.length === 0) {
          throw new Error('Unable to parse Patch string');
        }

        for (const block of blocks) {
          // wider diffs are built line-relative, so restore the line feeds
          for (const op of block.ops) {
            op[1] += EOL;
          }

          // Where does this block anchor in the document? Find the character
          // offset of its origin line by walking the head of the document.
          let walkLine = block.originLine;
          let walkCharacter = 0;
          if (walkLine > 0) {
            const headLines = documentContents.split(/\r?\n/g).filter((_l, i) => i < walkLine);
            for (const headLine of headLines) {
              walkCharacter += headLine.length + EOL.length;
            }
          }

          // 0 = kept text turned into a deletion, 1 = insertion, 2 = both (replace)
          let pendingKind = -1;
          let pendingFrom: Position = { line: 0, character: 0 };
          let pendingTo: Position = { line: 0, character: 0 };
          let pendingText = '';
          for (let i = 0; i < block.ops.length; i += 1) {
            const opStart = { line: walkLine, character: walkCharacter };
            // advance the caret over the text this op replaces
            for (let cursorAt = 0; cursorAt < block.ops[i][1].length; cursorAt += 1) {
              if (block.ops[i][1][cursorAt] !== '\n') {
                walkCharacter += 1;
              } else {
                walkCharacter = 0;
                walkLine += 1;
              }
            }

            switch (block.ops[i][0]) {
              case dmp.DIFF_DELETE:
                if (pendingKind === -1) {
                  pendingKind = 0;
                  pendingFrom = opStart;
                  pendingText = '';
                } else if (pendingKind !== 0) {
                  throw new Error('cannot format due to an internal error.');
                }
                pendingTo = { line: walkLine, character: walkCharacter };
                break;

              case dmp.DIFF_INSERT:
                if (pendingKind === -1) {
                  pendingKind = 1;
                  pendingFrom = opStart;
                } else if (pendingKind === 0) {
                  pendingKind = 2;
                }
                // the diff is relative to the original document, so put the
                // caret back where this run of ops started
                walkLine = opStart.line;
                walkCharacter = opStart.character;
                pendingText += block.ops[i][1];
                break;

              case dmp.DIFF_EQUAL:
                if (pendingKind !== -1) {
                  if (pendingKind === 1) {
                    textEdits.push(TextEdit.insert(pendingFrom, pendingText));
                  } else if (pendingKind === 0) {
                    textEdits.push(TextEdit.del(Range.create(pendingFrom, pendingTo)));
                  } else {
                    textEdits.push(TextEdit.replace(Range.create(pendingFrom, pendingTo), pendingText));
                  }
                  pendingKind = -1;
                }
                break;
            }
          }
          if (pendingKind !== -1) {
            if (pendingKind === 1) {
              textEdits.push(TextEdit.insert(pendingFrom, pendingText));
            } else if (pendingKind === 0) {
              textEdits.push(TextEdit.del(Range.create(pendingFrom, pendingTo)));
            } else {
              textEdits.push(TextEdit.replace(Range.create(pendingFrom, pendingTo), pendingText));
            }
          }
        }
      }

      if (textEdits.length) window.showInformationMessage(`Formatted with ${this.Id}`);
      return textEdits;
    } catch (error: any) {
      this.outputChannel.appendLine(`${'#'.repeat(10)} Formatting with ${this.Id} failed`);
      this.outputChannel.appendLine(error.message);

      let customError = `Formatting with ${this.Id} failed`;
      if (isNotInstalledError(error)) {
        customError = `${customError}: ${this.Id} module is not installed.`;
      }
      window.showWarningMessage(customError);

      if (token?.isCancellationRequested) {
        this.outputChannel.appendLine(`${'#'.repeat(10)} ${this.Id} formatting action is canceled on error`);
        if (filepath !== tempFile) {
          fs.promises.unlink(tempFile).catch(() => {});
        }
      }
      return [] as TextEdit[];
    } finally {
      if (filepath !== tempFile) {
        fs.promises.unlink(tempFile).catch(() => {});
      }
    }
  }

  protected createTempFile(document: TextDocument): Promise<string> {
    return getTempFileWithDocumentContents(document);
  }
}
