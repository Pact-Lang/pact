"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.activate = activate;
exports.deactivate = deactivate;
const vscode_1 = require("vscode");
const node_1 = require("vscode-languageclient/node");
let client;
function activate(context) {
    const config = vscode_1.workspace.getConfiguration('pact');
    const lspPath = config.get('lspPath', 'pact-lsp');
    const serverOptions = {
        command: lspPath,
        args: [],
    };
    const clientOptions = {
        documentSelector: [{ scheme: 'file', language: 'pact' }],
    };
    client = new node_1.LanguageClient('pact-lsp', 'PACT Language Server', serverOptions, clientOptions);
    client.start();
}
function deactivate() {
    if (!client)
        return undefined;
    return client.stop();
}
//# sourceMappingURL=extension.js.map