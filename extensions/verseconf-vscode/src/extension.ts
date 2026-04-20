import * as vscode from 'vscode';
import { LanguageClient, TransportKind } from 'vscode-languageclient/node';

let client: LanguageClient | undefined;

export function activate(context: vscode.ExtensionContext) {
    const config = vscode.workspace.getConfiguration('verseconf');
    const lspEnabled = config.get<boolean>('lsp.enabled', true);

    if (lspEnabled) {
        startLSP(context);
    }

    registerCommands(context);
}

function startLSP(context: vscode.ExtensionContext) {
    const config = vscode.workspace.getConfiguration('verseconf');
    const lspPath = config.get<string>('lsp.serverPath', '');

    let serverPath: string;

    if (lspPath && lspPath.trim() !== '') {
        serverPath = lspPath;
    } else {
        serverPath = context.asAbsolutePath('server/bin/verseconf-lsp.exe');
    }

    const serverOptions = {
        run: {
            command: serverPath,
            transport: TransportKind.stdio
        },
        debug: {
            command: serverPath,
            transport: TransportKind.stdio
        }
    };

    const clientOptions = {
        documentSelector: [
            { scheme: 'file', language: 'verseconf' },
            { scheme: 'file', pattern: '**/*.vcf' }
        ],
        synchronize: {
            fileEvents: vscode.workspace.createFileSystemWatcher('**/*.vcf')
        }
    };

    try {
        client = new LanguageClient('verseconf-lsp', 'VerseConf Language Server', serverOptions, clientOptions);

        client.start();

        vscode.window.showInformationMessage('VerseConf Language Server started.');
    } catch (err: any) {
        vscode.window.showWarningMessage(`VerseConf LSP error: ${err.message}`);
    }
}

function registerCommands(context: vscode.ExtensionContext) {
    context.subscriptions.push(
        vscode.commands.registerCommand('verseconf.format', async () => {
            const editor = vscode.window.activeTextEditor;
            if (!editor) { return; }

            try {
                await vscode.commands.executeCommand('editor.action.formatDocument');
            } catch (err: any) {
                vscode.window.showErrorMessage(`Format failed: ${err.message}`);
            }
        }),

        vscode.commands.registerCommand('verseconf.validate', async () => {
            const editor = vscode.window.activeTextEditor;
            if (!editor) { return; }

            vscode.window.showInformationMessage('Running VerseConf validation...');
        }),

        vscode.commands.registerCommand('verseconf.schema.generate', async () => {
            vscode.window.showInformationMessage('Schema generation coming soon!');
        })
    );
}

export function deactivate(): Thenable<void> | undefined {
    if (client) {
        return client.stop();
    }
    return undefined;
}
