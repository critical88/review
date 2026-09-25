import {
  type CancellationToken,
  Emitter,
  type Event,
  type Hover,
  type InlayHint,
  type InlayHintLabelPart,
  type InlayHintsProvider,
  type LanguageClient,
  type LinesTextDocument,
  type MarkupContent,
  Position,
  type Range,
  type SignatureHelp,
  workspace,
} from 'coc.nvim';

import * as parser from '../parsers';

export class TypeInlayHintsProvider implements InlayHintsProvider {
  private readonly _onDidChangeInlayHints = new Emitter<void>();
  public readonly onDidChangeInlayHints: Event<void> = this._onDidChangeInlayHints.event;

  constructor(private client: LanguageClient) {
    workspace.onDidChangeConfiguration((e) => {
      if (e.affectsConfiguration('pyright.inlayHints')) {
        this._onDidChangeInlayHints.fire();
      }
    });
    workspace.onDidChangeTextDocument((e) => {
      const doc = workspace.getDocument(e.bufnr);
      if (doc?.languageId === 'python') {
        this._onDidChangeInlayHints.fire();
      }
    });
  }

  async provideInlayHints(document: LinesTextDocument, range: Range, token: CancellationToken): Promise<InlayHint[]> {
    const inlayHints: InlayHint[] = [];

    const code = document.getText();
    const parsed = parser.parse(code);
    if (!parsed) return [];

    const walker = new parser.TypeInlayHintsWalker(parsed);
    walker.walk(parsed.parserOutput.parseTree);

    // Is the position inside the requested range? (start <= position <= end)
    const positionInSelectedRange = (position: Position): boolean => {
      if (position.line < range.start.line) return false;
      if (position.line === range.start.line && position.character < range.start.character) return false;
      if (position.line > range.end.line) return false;
      if (position.line === range.end.line && position.character > range.end.character) return false;
      return true;
    };

    const featureItems = walker.featureItems
      .filter((item) => workspace.getConfiguration('pyright').get(`inlayHints.${item.hintType}Types`, true))
      .filter((item) => {
        const startPosition = document.positionAt(item.startOffset);
        const endPosition = document.positionAt(item.endOffset);
        return positionInSelectedRange(startPosition) || positionInSelectedRange(endPosition);
      });
    if (featureItems.length === 0) return [];

    for (const item of featureItems) {
      const startPosition = document.positionAt(item.startOffset);
      const endPosition = document.positionAt(item.endOffset);
      // Ask pyright what sits under the cursor — both probes race a 200ms timer.
      const hover =
        item.hintType === 'parameter'
          ? null
          : await Promise.race([
              this.client.sendRequest<Hover>(
                'textDocument/hover',
                { textDocument: { uri: document.uri }, position: startPosition },
                token,
              ),
              new Promise<null>((resolve) => {
                setTimeout(() => {
                  resolve(null);
                }, 200);
              }),
            ]);
      const signatureHelp =
        item.hintType === 'parameter'
          ? await Promise.race([
              this.client.sendRequest<SignatureHelp>(
                'textDocument/signatureHelp',
                { textDocument: { uri: document.uri }, position: startPosition },
                token,
              ),
              new Promise<null>((resolve) => {
                setTimeout(() => {
                  resolve(null);
                }, 200);
              }),
            ])
          : null;

      let inlayHintLabelValue: string | undefined;
      switch (item.hintType) {
        case 'variable': {
          if (hover) {
            const contents = hover.contents as MarkupContent;
            if (contents.value.includes('(variable)') && !contents.value.includes('(variable) def')) {
              const firstIdx = contents.value.indexOf(': ');
              if (firstIdx > -1) {
                const text = contents.value
                  .substring(firstIdx + 2)
                  .split('\n')[0]
                  .trim();
                if (text !== 'Any' && !text.startsWith('Literal[')) {
                  inlayHintLabelValue = `: ${text}`;
                }
              }
            }
          }
          break;
        }
        case 'functionReturn': {
          if (hover) {
            const contents = hover.contents as MarkupContent;
            if (contents && (contents.value.includes('(function)') || contents.value.includes('(method)'))) {
              const retvalIdx = contents.value.indexOf('->') + 2;
              const text = contents.value.substring(retvalIdx).split('\n')[0].trim();
              inlayHintLabelValue = `-> ${text}`;
            }
          }
          break;
        }
        case 'parameter': {
          if (signatureHelp) {
            const sig = signatureHelp.signatures[0];
            if (
              typeof sig.activeParameter === 'number' &&
              sig.parameters &&
              sig.parameters.length >= sig.activeParameter
            ) {
              const param = sig.parameters[sig.activeParameter];
              if (typeof param.label === 'string') {
                inlayHintLabelValue = param.label;
              } else {
                const label = sig.label.substring(param.label[0], param.label[1]).split(':')[0];
                if (!label.startsWith('__')) {
                  inlayHintLabelValue = `${label}: `;
                }
              }
            }
          }
          break;
        }
        default:
          break;
      }
      if (!inlayHintLabelValue) {
        continue;
      }

      const inlayHintLabelPart: InlayHintLabelPart[] = [
        {
          value: inlayHintLabelValue,
        },
      ];

      let inlayHintPosition: Position | undefined;
      switch (item.hintType) {
        case 'variable':
          inlayHintPosition = Position.create(startPosition.line, endPosition.character + 1);
          break;
        case 'functionReturn':
          inlayHintPosition = endPosition;
          break;
        case 'parameter':
          inlayHintPosition = startPosition;
          break;
        default:
          break;
      }

      if (inlayHintPosition) {
        inlayHints.push({
          label: inlayHintLabelPart,
          position: inlayHintPosition,
          kind: item.hintType === 'parameter' ? 2 : 1,
          paddingLeft: item.hintType === 'functionReturn',
        });
      }
    }

    return inlayHints;
  }
}
