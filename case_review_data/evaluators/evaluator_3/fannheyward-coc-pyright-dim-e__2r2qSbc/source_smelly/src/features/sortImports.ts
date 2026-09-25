import { type OutputChannel, type Position, Range, TextEdit, Uri, commands, window, workspace } from 'coc.nvim';
import { spawn } from 'node:child_process';
import fs from 'node:fs';
import md5 from 'md5';
import { EOL, homedir } from 'node:os';
import path from 'node:path';
import * as iconv from 'iconv-lite';
import which from 'which';
import { PythonSettings } from '../configSettings';

type SortProvider = 'pyright' | 'isort' | 'ruff';

// One block of the sorter's unified diff, kept in the shape the diff library
// uses for parsed patches (diffs are [op, text] pairs).
type SorterHunk = { diffs: [number, string][]; start1: number; start2: number; length1: number; length2: number };

export async function sortImports(outputChannel: OutputChannel): Promise<void> {
  const doc = await workspace.document;
  if (doc?.filetype !== 'python' || doc.lineCount <= 1) {
    return;
  }

  const provider = workspace.getConfiguration('pyright').get<SortProvider>('organizeimports.provider', 'pyright');
  if (provider === 'pyright') {
    await commands.executeCommand('pyright.organizeimports');
    return;
  }

  try {
    const document = doc.textDocument;

    // Copy the buffer next to the original so the sorter picks up workspace
    // configuration. Don't create it in the temp folder: external utilities are
    // not able to find custom rules saved in a random disk location.
    const fsPath = Uri.parse(document.uri).fsPath;
    const ext = path.extname(fsPath);
    const fileName = `${fsPath.slice(0, -3)}${md5(document.uri)}${ext}`;
    const tempFile = await new Promise<string>((resolve, reject) => {
      fs.writeFile(fileName, document.getText(), (writeError) => {
        if (writeError) {
          reject(new Error(`Failed to create a temporary file, ${writeError.message}`));
        }
        resolve(fileName);
      });
    });

    // Which sorter binary to talk to, and with which arguments.
    const pythonSettings = PythonSettings.getInstance();
    const modulePath = provider === 'isort' ? pythonSettings.sortImports.path : pythonSettings.linting.ruffPath;
    const execPath = which.sync(workspace.expand(modulePath), { nothrow: true }) || '';
    let sortArgs: string[] = [];
    if (provider === 'isort') {
      sortArgs = ['--diff'];
      for (const item of pythonSettings.sortImports.args) {
        sortArgs.push(workspace.expand(item));
      }
    } else if (provider === 'ruff') {
      sortArgs = ['check', '--diff'].concat(['--quiet', '--select', 'I001']);
    }
    sortArgs.push(tempFile);

    outputChannel.appendLine(`${'#'.repeat(10)} sortImports`);
    outputChannel.appendLine(`execPath:   ${execPath}`);
    outputChannel.appendLine(`args:       ${sortArgs.join(' ')} `);

    // Run the sorter the same way the python execution service does for a bare
    // executable: unbuffered environment, both streams captured, and any
    // stderr at all treated as a failure.
    const execEnv = { ...process.env };
    execEnv.PYTHONUNBUFFERED = '1';
    if (!execEnv.PYTHONIOENCODING) {
      execEnv.PYTHONIOENCODING = 'utf-8';
    }
    let sorterStdout: string;
    try {
      const proc = spawn(execPath.startsWith('~/') ? execPath.replace('~', homedir()) : execPath, sortArgs, {
        env: execEnv,
      });
      const stdoutBuffers: Buffer[] = [];
      const stderrBuffers: Buffer[] = [];
      proc.stdout!.on('data', (data: Buffer) => stdoutBuffers.push(data));
      proc.stderr!.on('data', (data: Buffer) => stderrBuffers.push(data));
      sorterStdout = await new Promise<string>((resolve, reject) => {
        proc.once('close', () => {
          const stderr = stderrBuffers.length === 0 ? undefined : iconv.decode(Buffer.concat(stderrBuffers), 'utf8');
          if (stderr && stderr.length > 0) {
            reject(new Error(stderr));
          } else {
            resolve(iconv.decode(Buffer.concat(stdoutBuffers), 'utf8'));
          }
        });
        proc.once('error', (ex) => {
          console.error('once error:', ex);
          reject(ex);
        });
      });
    } finally {
      await fs.promises.unlink(tempFile);
    }

    // The sorter answers with a unified diff. Walk it directly and translate it
    // into workspace edits: first decode the hunks, then replay them against
    // the original contents of the document.
    const originalContents = doc.getDocumentContent();
    let diffText = sorterStdout;
    if (diffText.startsWith('---')) {
      diffText = diffText.substring(diffText.indexOf('@@'));
    }

    const textEdits: TextEdit[] = [];
    if (diffText.length > 0) {
      // Remove the text added by unified_diff
      // # Work around missing newline (http://bugs.python.org/issue2142).
      diffText = diffText.replace(/\\ No newline at end of file[\r\n]/, '');
      const dmp = require('diff-match-patch') as typeof import('diff-match-patch');
      const diffLines = diffText.split(/[\r\n]/);
      const hunks: SorterHunk[] = [];
      let linePointer = 0;
      while (linePointer < diffLines.length) {
        const m = diffLines[linePointer].match(/^@@ -(\d+),?(\d*) \+(\d+),?(\d*) @@$/);
        if (!m) {
          throw new Error(`Invalid patch string: ${diffLines[linePointer]}`);
        }
        const hunk: SorterHunk = { diffs: [], start1: 0, start2: 0, length1: 0, length2: 0 };
        hunks.push(hunk);
        hunk.start1 = parseInt(m[1], 10);
        if (m[2] === '') {
          hunk.start1 -= 1;
          hunk.length1 = 1;
        } else if (m[2] === '0') {
          hunk.length1 = 0;
        } else {
          hunk.start1 -= 1;
          hunk.length1 = parseInt(m[2], 10);
        }
        hunk.start2 = parseInt(m[3], 10);
        if (m[4] === '') {
          hunk.start2 -= 1;
          hunk.length2 = 1;
        } else if (m[4] === '0') {
          hunk.length2 = 0;
        } else {
          hunk.start2 -= 1;
          hunk.length2 = parseInt(m[4], 10);
        }
        linePointer += 1;
        while (linePointer < diffLines.length) {
          const sign = diffLines[linePointer].charAt(0);
          let bodyLine: string;
          try {
            // For some reason the patch generated by python files don't encode any characters
            // And this patch module (code from Google) is expecting the text to be encoded!!
            // Temporary solution, disable decoding
            // Issue #188
            bodyLine = diffLines[linePointer].substring(1);
          } catch (_ex) {
            throw new Error('Illegal escape in patch_fromText');
          }
          if (sign === '-') {
            hunk.diffs.push([dmp.DIFF_DELETE, bodyLine]);
          } else if (sign === '+') {
            hunk.diffs.push([dmp.DIFF_INSERT, bodyLine]);
          } else if (sign === ' ') {
            hunk.diffs.push([dmp.DIFF_EQUAL, bodyLine]);
          } else if (sign === '@') {
            break;
          } else if (sign === '') {
            // Blank line?  Whatever.
          } else {
            throw new Error(`Invalid patch mode '${sign}' in: ${bodyLine}`);
          }
          linePointer += 1;
        }
      }
      if (hunks.length === 0) {
        throw new Error('Unable to parse Patch string');
      }

      for (const hunk of hunks) {
        // line feeds are added back when replaying the diff below
        for (const diff of hunk.diffs) {
          diff[1] += EOL;
        }

        // Where in the original document does this hunk start?
        let cursorLine = hunk.start1;
        let cursorCharacter = 0;
        if (cursorLine > 0) {
          const prefixLines = originalContents.split(/\r?\n/g).filter((_l, i) => i < cursorLine);
          for (const l of prefixLines) {
            cursorCharacter += l.length + EOL.length;
          }
        }

        // Replay every op of the hunk: 0=delete, 1=insert, 2=delete+insert (replace).
        let pendingAction = -1;
        let pendingStart: Position = { line: 0, character: 0 };
        let pendingEnd: Position = { line: 0, character: 0 };
        let pendingText = '';
        const flushPendingEdit = () => {
          if (pendingAction === 1) {
            textEdits.push(TextEdit.insert(pendingStart, pendingText));
          } else if (pendingAction === 0) {
            textEdits.push(TextEdit.del(Range.create(pendingStart, pendingEnd)));
          } else if (pendingAction === 2) {
            textEdits.push(TextEdit.replace(Range.create(pendingStart, pendingEnd), pendingText));
          } else {
            textEdits.push({ range: Range.create(0, 0, 0, 0), newText: '' });
          }
        };
        for (let i = 0; i < hunk.diffs.length; i += 1) {
          const start = { line: cursorLine, character: cursorCharacter };
          // Compute the line/character after the op is applied.
          for (let curr = 0; curr < hunk.diffs[i][1].length; curr += 1) {
            if (hunk.diffs[i][1][curr] !== '\n') {
              cursorCharacter += 1;
            } else {
              cursorCharacter = 0;
              cursorLine += 1;
            }
          }

          switch (hunk.diffs[i][0]) {
            case dmp.DIFF_DELETE:
              if (pendingAction === -1) {
                pendingAction = 0;
                pendingStart = start;
                pendingText = '';
              } else if (pendingAction !== 0) {
                throw new Error('cannot format due to an internal error.');
              }
              pendingEnd = { line: cursorLine, character: cursorCharacter };
              break;

            case dmp.DIFF_INSERT:
              if (pendingAction === -1) {
                pendingAction = 1;
                pendingStart = start;
              } else if (pendingAction === 0) {
                pendingAction = 2;
              }
              // insert and replace edits are all relative to the original state
              // of the document, so inserts should reset the current line/character
              // position to the start.
              cursorLine = start.line;
              cursorCharacter = start.character;
              pendingText += hunk.diffs[i][1];
              break;

            case dmp.DIFF_EQUAL:
              if (pendingAction !== -1) {
                flushPendingEdit();
                pendingAction = -1;
              }
              break;
          }
        }
        if (pendingAction !== -1) {
          flushPendingEdit();
        }
      }
    }

    await doc.applyEdits(textEdits);

    outputChannel.appendLine(`${'#'.repeat(10)} sortImports Output ${'#'.repeat(10)}`);
    outputChannel.appendLine(sorterStdout);
  } catch (err) {
    let message = '';
    if (typeof err === 'string') {
      message = err;
    } else if (err instanceof Error) {
      message = err.message;
    }
    outputChannel.appendLine(`${'#'.repeat(10)} sortImports Error ${'#'.repeat(10)}`);
    outputChannel.appendLine(message);
    window.showErrorMessage('Failed to sort imports');
  }
}
