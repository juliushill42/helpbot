mod memory;
mod bus;
mod llm;
mod sandbox;
mod policy;

use std::io::{self, Write};
use llm::LLMClassifier;
use sandbox::Sandbox;
use policy::{PolicyEngine, Action};
use tracing_subscriber;

struct TitanCore {
    classifier: LLMClassifier,
    sandbox: Sandbox,
    policy: PolicyEngine,
    memory: memory::SystemMemory,
    bus: bus::MessageBus,
}

impl TitanCore {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // Initialize tracing
        tracing_subscriber::fmt::init();

        // Load policy from file or use default
        let policy = match PolicyEngine::from_file("policies/default.yaml") {
            Ok(p) => {
                tracing::info!("Loaded policy from policies/default.yaml");
                p
            }
            Err(e) => {
                tracing::warn!("Failed to load policy file: {}. Using default policy.", e);
                PolicyEngine::new(policy::PolicyConfig::default())
            }
        };

        // Setup sandbox with workspace access
        let mut sandbox = Sandbox::new()?;
        
        // Allow access to current directory and common workspace paths
        if let Ok(current_dir) = std::env::current_dir() {
            sandbox.allow_path(&current_dir);
            tracing::info!("Allowed workspace path: {}", current_dir.display());
        }
        
        // Allow access to home directory projects (if exists)
        if let Ok(home_dir) = std::env::var("HOME") {
            let workspace_path = format!("{}/workspace", home_dir);
            sandbox.allow_path(&workspace_path);
            let projects_path = format!("{}/projects", home_dir);
            sandbox.allow_path(&projects_path);
        }

        Ok(Self {
            classifier: LLMClassifier::new(),
            sandbox,
            policy,
            memory: memory::SystemMemory::new()?,
            bus: bus::MessageBus::new(),
        })
    }

    async fn process_command(&mut self, command: &str) -> Result<String, Box<dyn std::error::Error>> {
        tracing::info!("Processing command: {}", command);

        // Step 1: Classify with LLM
        let classification = self.classifier.classify(command).await;
        
        tracing::info!(
            "Classification: intent={:?}, risk={:.2}, confidence={:.2}, resources={:?}",
            classification.intent,
            classification.risk_score,
            classification.confidence,
            classification.affected_resources
        );

        // Step 2: Evaluate against policy
        let policy_action = self.policy.evaluate(&classification, command);
        
        tracing::info!("Policy decision: {:?}", policy_action);

        // Step 3: Execute based on policy decision
        match policy_action {
            Action::Allow => {
                self.execute_command(command, &classification).await
            }
            Action::Deny => {
                let msg = format!(
                    "❌ DENIED: Risk score {:.2} exceeds threshold\nIntent: {:?}\nReason: {}",
                    classification.risk_score,
                    classification.intent,
                    classification.reasoning
                );
                tracing::warn!("Command denied: {}", command);
                Ok(msg)
            }
            Action::RequireApproval => {
                let msg = format!(
                    "⚠️  APPROVAL REQUIRED:\n  Intent: {:?}\n  Risk Score: {:.2}\n  Confidence: {:.2}\n  Resources: {:?}\n  Reasoning: {}",
                    classification.intent,
                    classification.risk_score,
                    classification.confidence,
                    classification.affected_resources,
                    classification.reasoning
                );
                Ok(msg)
            }
            Action::Log => {
                tracing::info!("Logged operation: {}", command);
                self.execute_command(command, &classification).await
            }
        }
    }

    async fn execute_command(
        &mut self,
        command: &str,
        classification: &llm::Classification,
    ) -> Result<String, Box<dyn std::error::Error>> {
        use llm::Intent;

        let result = match classification.intent {
            Intent::FileRead => {
                self.handle_file_read(command, &classification.affected_resources)
            }
            Intent::FileWrite => {
                self.handle_file_write(command, &classification.affected_resources)
            }
            Intent::FileDelete => {
                self.handle_file_delete(command, &classification.affected_resources)
            }
            Intent::ProcessExecute => {
                self.handle_process_execute(command)
            }
            Intent::NetworkAccess => {
                self.handle_network_access(command)
            }
            Intent::SystemConfig => {
                Ok("System configuration commands require manual execution".to_string())
            }
            Intent::PackageInstall => {
                Ok(format!("Package installation detected. Please run manually: {}", command))
            }
            Intent::Unknown => {
                Ok(format!("Unknown intent. Command: {}", command))
            }
        };

        // Record to memory
        if let Ok(ref response) = result {
            self.memory.record_event(command, response)?;
            self.bus.publish("command_executed", &serde_json::json!({
                "command": command,
                "intent": format!("{:?}", classification.intent),
                "risk_score": classification.risk_score,
                "result": response
            }));
        }

        result
    }

    fn handle_file_read(
        &self,
        _command: &str,
        resources: &[String],
    ) -> Result<String, Box<dyn std::error::Error>> {
        if resources.is_empty() {
            return Ok("No file path specified".to_string());
        }

        let path = &resources[0];
        match self.sandbox.safe_read(path) {
            Ok((contents, hash)) => {
                let preview = if contents.len() > 200 {
                    format!("{}... [truncated]", String::from_utf8_lossy(&contents[..200]))
                } else {
                    String::from_utf8_lossy(&contents).to_string()
                };
                Ok(format!(
                    "✓ Read file: {}\nSize: {} bytes\nHash: {}\nPreview:\n{}",
                    path,
                    contents.len(),
                    hash,
                    preview
                ))
            }
            Err(e) => Ok(format!("❌ Failed to read file: {}", e)),
        }
    }

    fn handle_file_write(
        &self,
        command: &str,
        resources: &[String],
    ) -> Result<String, Box<dyn std::error::Error>> {
        if resources.is_empty() {
            return Ok("No file path specified".to_string());
        }

        let path = &resources[0];
        // For demo, write the command as content (in production, parse actual content)
        let content = format!("Generated by: {}", command);
        
        match self.sandbox.safe_write(path, content.as_bytes()) {
            Ok(hash) => Ok(format!(
                "✓ Wrote to file: {}\nSize: {} bytes\nHash: {}",
                path,
                content.len(),
                hash
            )),
            Err(e) => Ok(format!("❌ Failed to write file: {}", e)),
        }
    }

    fn handle_file_delete(
        &self,
        _command: &str,
        resources: &[String],
    ) -> Result<String, Box<dyn std::error::Error>> {
        if resources.is_empty() {
            return Ok("No file path specified".to_string());
        }

        let path = &resources[0];
        match self.sandbox.safe_delete(path) {
            Ok(hash) => Ok(format!(
                "✓ Deleted file (moved to trash): {}\nHash: {}\nTrash location: {}",
                path,
                hash,
                self.sandbox.get_trash_dir().display()
            )),
            Err(e) => Ok(format!("❌ Failed to delete file: {}", e)),
        }
    }

    fn handle_process_execute(&self, command: &str) -> Result<String, Box<dyn std::error::Error>> {
        Ok(format!(
            "⚙️  Process execution requested: {}\n(Execution disabled in demo mode for safety)",
            command
        ))
    }

    fn handle_network_access(&self, command: &str) -> Result<String, Box<dyn std::error::Error>> {
        Ok(format!(
            "🌐 Network access requested: {}\n(Network operations disabled in demo mode for safety)",
            command
        ))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("╔═══════════════════════════════════════════════════════════╗");
    println!("║   TITAN PRODUCTION SECURITY FRAMEWORK v0.2.0             ║");
    println!("║   LLM Classification + Policy Engine + Sandbox            ║");
    println!("╚═══════════════════════════════════════════════════════════╝");
    println!();
    println!("🔒 Security Features:");
    println!("   • AI-powered intent classification (OpenAI/Ollama)");
    println!("   • File system sandboxing with hash verification");
    println!("   • YAML-based policy engine");
    println!("   • Rate limiting and time-based rules");
    println!("   • Recoverable deletes with trash system");
    println!("   • Audit logging with SHA256 hashing");
    println!();
    println!("📋 Policy loaded from: policies/default.yaml");
    println!();
    println!("💡 Test Commands:");
    println!("   - cat test.txt           (FileRead - low risk)");
    println!("   - write data.json        (FileWrite - requires approval)");
    println!("   - rm temp.txt            (FileDelete - requires approval)");
    println!("   - curl api.example.com   (NetworkAccess - logged)");
    println!("   - npm install express    (PackageInstall - requires approval)");
    println!("   - rm -rf /               (Recursive delete - DENIED)");
    println!("   - cat /etc/passwd        (System path - DENIED)");
    println!();
    println!("Type 'exit' to shutdown\n");

    let mut core = TitanCore::new()?;

    loop {
        print!("TITAN> ");
        io::stdout().flush()?;
        
        let mut user_input = String::new();
        io::stdin().read_line(&mut user_input)?;
        let user_input = user_input.trim();

        if user_input == "exit" || user_input == "quit" {
            println!("\n🔴 TITAN CORE SHUTTING DOWN...");
            println!("   All operations have been logged to: titan.db");
            println!("   Trash directory: {}", core.sandbox.get_trash_dir().display());
            break;
        }

        if user_input.is_empty() {
            continue;
        }

        // Process command through security pipeline
        match core.process_command(user_input).await {
            Ok(response) => {
                println!("\n{}\n", response);
                
                // If approval is required, ask user
                if response.contains("APPROVAL REQUIRED") {
                    print!("Approve execution? (y/n): ");
                    io::stdout().flush()?;
                    let mut approval = String::new();
                    io::stdin().read_line(&mut approval)?;
                    
                    if approval.trim().to_lowercase() == "y" {
                        println!("✓ APPROVED - Command would be executed\n");
                        // In production, would actually execute here
                    } else {
                        println!("✗ REJECTED - Command not executed\n");
                    }
                }
            }
            Err(e) => {
                println!("❌ Error: {}\n", e);
            }
        }
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_titan_core_creation() {
        let result = TitanCore::new();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_safe_command_processing() {
        let mut core = TitanCore::new().unwrap();
        let result = core.process_command("cat README.md").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_dangerous_command_blocked() {
        let mut core = TitanCore::new().unwrap();
        let result = core.process_command("rm -rf /").await;
        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(response.contains("DENIED") || response.contains("APPROVAL"));
    }
}
