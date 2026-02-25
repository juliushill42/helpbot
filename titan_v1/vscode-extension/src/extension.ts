import * as vscode from 'vscode';
import { TitanBridge } from './bridge';

let bridge: TitanBridge;

export function activate(context: vscode.ExtensionContext) {
    console.log('TITAN Orchestrator Extension Activated');
    
    bridge = new TitanBridge();
    
    let syncCmd = vscode.commands.registerCommand('titan.sync', async () => {
        try {
            await bridge.connect();
        } catch (error) {
            vscode.window.showErrorMessage(`Failed to connect to Titan Core: ${error}`);
        }
    });
    
    let approveCmd = vscode.commands.registerCommand('titan.approve', async () => {
        // Placeholder for batch approval UI
        vscode.window.showInformationMessage('Batch approval UI coming soon...');
    });
    
    let viewQueueCmd = vscode.commands.registerCommand('titan.viewQueue', async () => {
        // Placeholder for queue viewer
        vscode.window.showInformationMessage('Queue viewer coming soon...');
    });
    
    context.subscriptions.push(syncCmd, approveCmd, viewQueueCmd);
}

export function deactivate() {
    if (bridge) {
        bridge.disconnect();
    }
}
