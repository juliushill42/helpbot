import * as net from 'net';
import * as vscode from 'vscode';

interface ApprovalRequest {
    id: number;
    instruction: string;
    risk_score: number;
}

export class TitanBridge {
    private client: net.Socket;
    private connected: boolean = false;
    
    constructor() {
        this.client = new net.Socket();
    }
    
    async connect(): Promise<void> {
        return new Promise((resolve, reject) => {
            this.client.connect(8080, '127.0.0.1', () => {
                this.connected = true;
                vscode.window.showInformationMessage('✓ TITAN CORE: Linked to VS Code');
                resolve();
            });
            
            this.client.on('error', (err) => {
                this.connected = false;
                reject(err);
            });
        });
    }
    
    async requestApproval(request: ApprovalRequest): Promise<string> {
        const riskLabel = request.risk_score < 0.3 ? 'LOW' : 
                         request.risk_score < 0.7 ? 'MEDIUM' : 'HIGH';
        
        const selection = await vscode.window.showInformationMessage(
            `TITAN ACTION REQUIRED [${riskLabel} RISK]:\n${request.instruction}`,
            { modal: true },
            'APPROVE',
            'DENY',
            'DEFER'
        );
        
        if (selection === 'APPROVE') {
            this.sendApproval(request.id, 'AUTHORIZED');
            return 'APPROVED';
        } else if (selection === 'DENY') {
            this.sendApproval(request.id, 'REJECTED');
            return 'DENIED';
        }
        
        return 'DEFERRED';
    }
    
    private sendApproval(id: number, status: string): void {
        if (this.connected) {
            const message = JSON.stringify({ 
                id, 
                status, 
                timestamp: new Date().toISOString() 
            });
            this.client.write(message);
        }
    }
    
    disconnect(): void {
        this.client.destroy();
        this.connected = false;
    }
}
