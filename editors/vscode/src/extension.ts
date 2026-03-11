import { workspace, ExtensionContext } from 'vscode';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
} from 'vscode-languageclient/node';

let client: LanguageClient;

export function activate(context: ExtensionContext) {
    const config = workspace.getConfiguration('pact');
    const lspPath = config.get<string>('lspPath', 'pact-lsp');

    const serverOptions: ServerOptions = {
        command: lspPath,
        args: [],
    };

    const clientOptions: LanguageClientOptions = {
        documentSelector: [{ scheme: 'file', language: 'pact' }],
    };

    client = new LanguageClient(
        'pact-lsp',
        'PACT Language Server',
        serverOptions,
        clientOptions
    );

    client.start();
}

export function deactivate(): Thenable<void> | undefined {
    if (!client) return undefined;
    return client.stop();
}
