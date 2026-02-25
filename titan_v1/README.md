# Titan Orchestration System v0.2.0

AI Security Framework with LLM Classification, Policy Governance, and Sandboxed Execution

## Quick Start

### Option 1: Double-Click Launch (Easiest)
1. Double-click `launch_titan.command`
2. System will auto-build if needed
3. Titan starts monitoring on port 8080

### Option 2: Manual Build
```bash
cd host
cargo build --release
./target/release/titan-host
```

### Option 3: Development Mode
```bash
cd host
cargo run
```

## Configuration

### Set OpenAI API Key (Optional)
```bash
export OPENAI_API_KEY="sk-your-key-here"
```

Without API key, Titan uses Ollama local LLM fallback.

### Install Ollama (For Offline Operation)
```bash
brew install ollama
ollama pull llama3
```

### Customize Security Policy
Edit `policies/default.yaml` to adjust:
- Risk thresholds (auto-approve, require-approval, auto-deny)
- Allowed/denied paths
- Rate limits
- Business hours restrictions

## Architecture

```
User Input → LLM Classifier → Policy Engine → Sandbox → Execution
              ↓ fallback         ↓ YAML rules    ↓ SHA256    ↓ Audit
           Offline Pattern    Risk-based       Safe Ops   Memory DB
```

## Components

- **LLM Engine** (`host/src/llm.rs`): OpenAI/Ollama intent classification
- **Policy Engine** (`host/src/policy.rs`): YAML-based security rules
- **Sandbox** (`host/src/sandbox.rs`): File system protection with hashing
- **Memory** (`host/src/memory.rs`): SQLite audit trail with WAL mode
- **Event Bus** (`host/src/bus.rs`): JSON-RPC bridge on port 8080

## Built Binary Location

After build: `host/target/release/titan-host`

## VS Code Extension

```bash
cd vscode-extension
npm install
npm run compile
# Press F5 in VS Code
# Run command: "Titan: Connect to Core"
```

## Files

- `launch_titan.command` - Double-click launcher
- `policies/default.yaml` - Security policy configuration
- `host/target/release/titan-host` - Compiled executable
- `titan_memory.db` - Audit log database (created on first run)

## Security Features

✅ LLM-powered intent classification
✅ YAML-based policy rules (15 default rules)
✅ SHA256 hash-verified operations
✅ Recoverable deletes (files moved to temp)
✅ Rate limiting (60 ops/min, 10 deletes/hour)
✅ Business hours enforcement
✅ System path blocking (/etc, /usr, /.ssh)
✅ Human-in-the-loop approval gates

## Example Commands Titan Handles

```bash
# Safe read (auto-approved)
cat README.md

# File write (requires approval)
echo "data" > output.txt

# Network access (logged)
curl https://api.example.com

# Dangerous operation (denied)
rm -rf /

# System file access (blocked by sandbox)
cat /etc/passwd
```

## License

Production-ready AI security infrastructure.
