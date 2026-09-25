import { DiagnosticSink } from '@zzzen/pyright-internal/dist/common/diagnosticSink';
import { printParseNodeType } from '@zzzen/pyright-internal/dist/analyzer/parseTreeUtils';
import {
  type ClassNode,
  type FunctionNode,
  type ParseNode,
  ParseNodeType,
  type SuiteNode,
} from '@zzzen/pyright-internal/dist/parser/parseNodes';
import { getChildNodes } from '@zzzen/pyright-internal/dist/parser/parseTreeUtils';
import { ParseOptions, type ParseFileResults, Parser } from '@zzzen/pyright-internal/dist/parser/parser';
import * as child_process from 'node:child_process';
import { type Terminal, Uri, window, workspace } from 'coc.nvim';
import path from 'node:path';
import { PythonSettings } from './configSettings';
import type { TestingFramework } from './types';

let terminal: Terminal | undefined;

const framework = workspace.getConfiguration('pyright').get<TestingFramework>('testing.provider', 'unittest');

function pythonSupportsPathFinder(pythonPath: string) {
  try {
    const pythonProcess = child_process.spawnSync(
      pythonPath,
      ['-c', 'from sys import version_info; exit(0) if (version_info[0] >= 3 and version_info[1] >= 4) else exit(1)'],
      { encoding: 'utf8' },
    );
    if (pythonProcess.error) return false;
    return pythonProcess.status === 0;
  } catch (_ex) {
    return false;
  }
}

function validPythonModule(pythonPath: string, moduleName: string) {
  const pythonArgs = pythonSupportsPathFinder(pythonPath)
    ? ['-c', `from importlib.machinery import PathFinder; assert PathFinder.find_spec("${moduleName}") is not None`]
    : ['-m', moduleName, '--help'];
  try {
    const pythonProcess = child_process.spawnSync(pythonPath, pythonArgs, { encoding: 'utf8' });
    if (pythonProcess.error) return false;
    return pythonProcess.status === 0;
  } catch (_ex) {
    return false;
  }
}

async function runTest(uri: string, testFunction?: string) {
  const workspaceUri = Uri.parse(workspace.root).toString();
  const relativeFileUri = uri.replace(`${workspaceUri}/`, '');
  let testFile = '';
  if (framework === 'pytest') {
    testFile = relativeFileUri.split('/').join(path.sep);
  } else {
    testFile = relativeFileUri.replace(/.py$/, '').split('/').join('.');
  }

  const pythonPath = PythonSettings.getInstance().pythonPath;
  const exists = validPythonModule(pythonPath, framework);
  if (!exists) return window.showErrorMessage(`${framework} does not exist!`);

  if (terminal) {
    if (terminal.bufnr) {
      await workspace.nvim.command(`bd! ${terminal.bufnr}`);
    }
    terminal.dispose();
    terminal = undefined;
  }

  terminal = await window.createTerminal({ name: framework, cwd: workspace.root });
  const args: string[] = [];

  const testArgs = workspace.getConfiguration('pyright').get<string[]>(`testing.${framework}Args`, []);
  if (testArgs) {
    if (Array.isArray(testArgs)) {
      args.push(...testArgs);
    }
  }

  // MEMO: pytest is string concatenation with '::'
  // MEMO: unittest is string concatenation with '.'
  const sep = framework === 'pytest' ? '::' : '.';
  args.push(testFunction ? testFile + sep + testFunction : testFile);

  terminal.sendText(`${pythonPath} -m ${framework} ${args.join(' ')}`);
}

export async function runFileTest() {
  const { document } = await workspace.getCurrentState();

  const fileName = path.basename(Uri.parse(document.uri).fsPath);
  if (document.languageId !== 'python' || (!fileName.startsWith('test_') && !fileName.endsWith('_test.py'))) {
    return window.showErrorMessage('This file is not a python test file!');
  }

  runTest(document.uri);
}

export async function runSingleTest() {
  const { document, position } = await workspace.getCurrentState();
  const fileName = path.basename(Uri.parse(document.uri).fsPath);
  if (document.languageId !== 'python' || (!fileName.startsWith('test_') && !fileName.endsWith('_test.py'))) {
    return window.showErrorMessage('This file is not a python test file!');
  }

  // Parse the python source in place.
  const parseOptions = new ParseOptions();
  const diagSink = new DiagnosticSink();
  const pythonParser = new Parser();
  let parsed: ParseFileResults | undefined;
  try {
    parsed = pythonParser.parseSourceFile(document.getText(), parseOptions, diagSink);
  } catch (_e) {}
  if (!parsed) return window.showErrorMessage('Test not found');

  // Collect the test functions straight from the tree: every function whose
  // name starts with test_ inside a suite (or at module level for pytest).
  const featureItems: { value: string; startOffset: number; endOffset: number }[] = [];
  const collectTestFunctions = (node: ParseNode): void => {
    if (node.nodeType === ParseNodeType.Function) {
      const fNode = node as FunctionNode;
      if (fNode.d.name.d.value.startsWith('test_')) {
        if (fNode.parent && printParseNodeType(fNode.parent.nodeType) === 'Suite') {
          let fullyQualifiedTestName = '';
          let currentNode: FunctionNode | ParseNode | undefined = fNode;
          let parentSuiteNode = currentNode?.parent as SuiteNode;
          while (parentSuiteNode.parent && printParseNodeType(parentSuiteNode.parent.nodeType) === 'Class') {
            const classNode = parentSuiteNode.parent as ClassNode;

            let combineString: string | undefined;
            if (framework === 'unittest') {
              combineString = '.';
            } else if (framework === 'pytest') {
              combineString = '::';
            }
            fullyQualifiedTestName = classNode.d.name.d.value + combineString + fullyQualifiedTestName;
            currentNode = currentNode?.parent?.parent;
            parentSuiteNode = currentNode?.parent as SuiteNode;
          }
          featureItems.push({
            value: fullyQualifiedTestName + fNode.d.name.d.value,
            startOffset: fNode.start,
            endOffset: fNode.start + fNode.length - 1,
          });
        } else {
          if (framework === 'pytest') {
            featureItems.push({
              value: fNode.d.name.d.value,
              startOffset: fNode.start,
              endOffset: fNode.start + fNode.length - 1,
            });
          }
        }
      }
    }
    for (const child of getChildNodes(node)) {
      if (child) {
        collectTestFunctions(child);
      }
    }
  };
  collectTestFunctions(parsed.parserOutput.parseTree);

  let testFunction: string | undefined;
  for (const item of featureItems) {
    const itemStartPosition = document.positionAt(item.startOffset);
    const itemEndPosition = document.positionAt(item.endOffset);
    if (position.line >= itemStartPosition.line && position.line <= itemEndPosition.line) {
      testFunction = item.value;
    }
  }

  if (!testFunction) return window.showErrorMessage('Test not found');

  // Everything the terminal launcher used to do for the single test, in place.
  const workspaceUri = Uri.parse(workspace.root).toString();
  const relativeFileUri = document.uri.replace(`${workspaceUri}/`, '');
  let testFile = '';
  if (framework === 'pytest') {
    testFile = relativeFileUri.split('/').join(path.sep);
  } else {
    testFile = relativeFileUri.replace(/.py$/, '').split('/').join('.');
  }

  const pythonPath = PythonSettings.getInstance().pythonPath;

  // Does the testing framework resolve on this interpreter?
  // First: does python even know about importlib.machinery.PathFinder?
  let pySupportsPathFinder = false;
  try {
    const versionProcess = child_process.spawnSync(
      pythonPath,
      ['-c', 'from sys import version_info; exit(0) if (version_info[0] >= 3 and version_info[1] >= 4) else exit(1)'],
      { encoding: 'utf8' },
    );
    if (versionProcess.error) {
      pySupportsPathFinder = false;
    } else {
      pySupportsPathFinder = versionProcess.status === 0;
    }
  } catch (_ex) {
    pySupportsPathFinder = false;
  }

  let moduleProbeArgs: string[];
  if (pySupportsPathFinder) {
    moduleProbeArgs = [
      '-c',
      `from importlib.machinery import PathFinder; assert PathFinder.find_spec("${framework}") is not None`,
    ];
  } else {
    moduleProbeArgs = ['-m', framework, '--help'];
  }
  let moduleUsable = false;
  try {
    const moduleProcess = child_process.spawnSync(pythonPath, moduleProbeArgs, { encoding: 'utf8' });
    if (moduleProcess.error) {
      moduleUsable = false;
    } else {
      moduleUsable = moduleProcess.status === 0;
    }
  } catch (_ex) {
    moduleUsable = false;
  }
  if (!moduleUsable) return window.showErrorMessage(`${framework} does not exist!`);

  if (terminal) {
    if (terminal.bufnr) {
      await workspace.nvim.command(`bd! ${terminal.bufnr}`);
    }
    terminal.dispose();
    terminal = undefined;
  }

  terminal = await window.createTerminal({ name: framework, cwd: workspace.root });
  const args: string[] = [];

  const testArgs = workspace.getConfiguration('pyright').get<string[]>(`testing.${framework}Args`, []);
  if (testArgs && Array.isArray(testArgs)) {
    for (const item of testArgs) {
      args.push(item);
    }
  }

  // MEMO: pytest is string concatenation with '::'
  // MEMO: unittest is string concatenation with '.'
  let sep: string;
  if (framework === 'pytest') {
    sep = '::';
  } else {
    sep = '.';
  }
  if (testFunction) {
    args.push(testFile + sep + testFunction);
  } else {
    args.push(testFile);
  }

  terminal.sendText(`${pythonPath} -m ${framework} ${args.join(' ')}`);
}
