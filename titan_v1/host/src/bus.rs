use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub id: u64,
    pub instruction: String,
    pub risk_score: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    jsonrpc: String,
    status: String,
    payload: Option<String>,
}

pub struct EventBus {
    pending_approvals: Arc<Mutex<Vec<ApprovalRequest>>>,
    approval_tx: mpsc::Sender<(u64, String)>,
    approval_rx: Arc<Mutex<mpsc::Receiver<(u64, String)>>>,
}

impl EventBus {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel(100);
        Self {
            pending_approvals: Arc::new(Mutex::new(Vec::new())),
            approval_tx: tx,
            approval_rx: Arc::new(Mutex::new(rx)),
        }
    }
    
    pub async fn queue_approval(&self, request: ApprovalRequest) {
        let mut queue = self.pending_approvals.lock().await;
        queue.push(request);
    }
    
    pub async fn start_bridge(&self) -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind("127.0.0.1:8080").await?;
        println!("[TITAN BRIDGE] Operational on Port 8080. Awaiting VS Code Handshake...");
        
        let pending = Arc::clone(&self.pending_approvals);
        
        loop {
            let (mut socket, _) = listener.accept().await?;
            let pending_clone = Arc::clone(&pending);
            
            tokio::spawn(async move {
                let mut buffer = [0; 1024];
                loop {
                    match socket.read(&mut buffer).await {
                        Ok(0) => break,
                        Ok(n) => {
                            let request = String::from_utf8_lossy(&buffer[..n]);
                            println!("[TITAN BRIDGE] Command Received: {}", request);
                            
                            // Get pending approvals
                            let queue = pending_clone.lock().await;
                            let response = JsonRpcResponse {
                                jsonrpc: "2.0".to_string(),
                                status: "ACKNOWLEDGED".to_string(),
                                payload: Some(format!("PENDING_APPROVALS: {}", queue.len())),
                            };
                            
                            let response_json = serde_json::to_string(&response).unwrap();
                            if let Err(e) = socket.write_all(response_json.as_bytes()).await {
                                eprintln!("Bridge Write Error: {}", e);
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
        }
    }
}
