use serde::{Deserialize, Serialize};
use regex::Regex;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Intent {
    FileRead,
    FileWrite,
    FileDelete,
    ProcessExecute,
    NetworkAccess,
    SystemConfig,
    PackageInstall,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Classification {
    pub intent: Intent,
    pub risk_score: f64,        // 0.0 = safe, 1.0 = critical
    pub confidence: f64,         // 0.0 = uncertain, 1.0 = certain
    pub affected_resources: Vec<String>,
    pub reasoning: String,
}

pub struct LLMClassifier {
    openai_api_key: Option<String>,
    ollama_endpoint: String,
    client: reqwest::Client,
}

impl LLMClassifier {
    pub fn new() -> Self {
        let openai_api_key = std::env::var("OPENAI_API_KEY").ok();
        let ollama_endpoint = std::env::var("OLLAMA_ENDPOINT")
            .unwrap_or_else(|_| "http://localhost:11434/api/generate".to_string());
        
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap();

        Self {
            openai_api_key,
            ollama_endpoint,
            client,
        }
    }

    pub async fn classify(&self, command: &str) -> Classification {
        // Try OpenAI first if available
        if let Some(api_key) = &self.openai_api_key {
            if let Ok(classification) = self.classify_with_openai(command, api_key).await {
                return classification;
            }
        }

        // Try Ollama as fallback
        if let Ok(classification) = self.classify_with_ollama(command).await {
            return classification;
        }

        // Final fallback to pattern-based classification
        self.classify_offline(command)
    }

    async fn classify_with_openai(&self, command: &str, api_key: &str) -> Result<Classification, Box<dyn std::error::Error>> {
        let prompt = format!(
            r#"Analyze this command and classify it with security assessment:
Command: {}

Respond in JSON format:
{{
  "intent": "<FileRead|FileWrite|FileDelete|ProcessExecute|NetworkAccess|SystemConfig|PackageInstall|Unknown>",
  "risk_score": <0.0-1.0>,
  "confidence": <0.0-1.0>,
  "affected_resources": ["<resource1>", "<resource2>"],
  "reasoning": "<brief explanation>"
}}

Risk scoring:
- 0.0-0.3: Safe operations (read logs, list files)
- 0.4-0.6: Moderate risk (write files, install packages)
- 0.7-1.0: High risk (delete system files, network access, execute processes)"#,
            command
        );

        #[derive(Serialize)]
        struct OpenAIRequest {
            model: String,
            messages: Vec<Message>,
            temperature: f32,
        }

        #[derive(Serialize)]
        struct Message {
            role: String,
            content: String,
        }

        #[derive(Deserialize)]
        struct OpenAIResponse {
            choices: Vec<Choice>,
        }

        #[derive(Deserialize)]
        struct Choice {
            message: MessageContent,
        }

        #[derive(Deserialize)]
        struct MessageContent {
            content: String,
        }

        let request = OpenAIRequest {
            model: "gpt-4o-mini".to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: prompt,
            }],
            temperature: 0.0,
        };

        let response = self.client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        let openai_response: OpenAIResponse = response.json().await?;
        let content = &openai_response.choices[0].message.content;
        
        // Extract JSON from response (handle markdown code blocks)
        let json_str = if content.contains("```json") {
            content
                .split("```json")
                .nth(1)
                .and_then(|s| s.split("```").next())
                .unwrap_or(content)
                .trim()
        } else if content.contains("```") {
            content
                .split("```")
                .nth(1)
                .unwrap_or(content)
                .trim()
        } else {
            content.trim()
        };

        let classification: Classification = serde_json::from_str(json_str)?;
        Ok(classification)
    }

    async fn classify_with_ollama(&self, command: &str) -> Result<Classification, Box<dyn std::error::Error>> {
        let prompt = format!(
            r#"Analyze this command and classify it with security assessment:
Command: {}

Respond ONLY with valid JSON (no markdown):
{{
  "intent": "<FileRead|FileWrite|FileDelete|ProcessExecute|NetworkAccess|SystemConfig|PackageInstall|Unknown>",
  "risk_score": <0.0-1.0>,
  "confidence": <0.0-1.0>,
  "affected_resources": ["<resource1>", "<resource2>"],
  "reasoning": "<brief explanation>"
}}"#,
            command
        );

        #[derive(Serialize)]
        struct OllamaRequest {
            model: String,
            prompt: String,
            stream: bool,
        }

        #[derive(Deserialize)]
        struct OllamaResponse {
            response: String,
        }

        let request = OllamaRequest {
            model: "llama2".to_string(),
            prompt,
            stream: false,
        };

        let response = self.client
            .post(&self.ollama_endpoint)
            .json(&request)
            .send()
            .await?;

        let ollama_response: OllamaResponse = response.json().await?;
        
        // Extract JSON from response
        let json_str = if ollama_response.response.contains("```json") {
            ollama_response.response
                .split("```json")
                .nth(1)
                .and_then(|s| s.split("```").next())
                .unwrap_or(&ollama_response.response)
                .trim()
        } else if ollama_response.response.contains("{") {
            let start = ollama_response.response.find('{').unwrap();
            let end = ollama_response.response.rfind('}').unwrap();
            &ollama_response.response[start..=end]
        } else {
            &ollama_response.response
        };

        let classification: Classification = serde_json::from_str(json_str)?;
        Ok(classification)
    }

    fn classify_offline(&self, command: &str) -> Classification {
        let cmd_lower = command.to_lowercase();
        
        // Pattern-based classification
        let (intent, risk_score, resources) = if cmd_lower.contains("rm ") 
            || cmd_lower.contains("delete") 
            || cmd_lower.contains("unlink") {
            (Intent::FileDelete, 0.9, extract_paths(command))
        } else if cmd_lower.contains("curl") 
            || cmd_lower.contains("wget") 
            || cmd_lower.contains("http") 
            || cmd_lower.contains("fetch") {
            (Intent::NetworkAccess, 0.7, extract_urls(command))
        } else if cmd_lower.contains("write") 
            || cmd_lower.contains("echo >") 
            || cmd_lower.contains("touch") 
            || cmd_lower.contains("mkdir") {
            (Intent::FileWrite, 0.5, extract_paths(command))
        } else if cmd_lower.contains("cat ") 
            || cmd_lower.contains("read") 
            || cmd_lower.contains("ls") 
            || cmd_lower.contains("find") {
            (Intent::FileRead, 0.2, extract_paths(command))
        } else if cmd_lower.contains("npm install") 
            || cmd_lower.contains("pip install") 
            || cmd_lower.contains("cargo install") 
            || cmd_lower.contains("apt install") {
            (Intent::PackageInstall, 0.6, extract_package_names(command))
        } else if cmd_lower.contains("sudo") 
            || cmd_lower.contains("chmod") 
            || cmd_lower.contains("chown") 
            || cmd_lower.contains("systemctl") {
            (Intent::SystemConfig, 0.8, vec![command.to_string()])
        } else if cmd_lower.contains("exec") 
            || cmd_lower.contains("sh ") 
            || cmd_lower.contains("bash ") 
            || cmd_lower.contains("python ") 
            || cmd_lower.contains("node ") {
            (Intent::ProcessExecute, 0.7, extract_paths(command))
        } else {
            (Intent::Unknown, 0.5, vec![])
        };

        Classification {
            intent,
            risk_score,
            confidence: 0.7, // Pattern matching is fairly confident
            affected_resources: resources,
            reasoning: "Offline pattern-based classification".to_string(),
        }
    }
}

fn extract_paths(command: &str) -> Vec<String> {
    let path_regex = Regex::new(r#"(?:[~/.])?[\w/.-]+/[\w/.-]+|[\w.-]+\.\w+"#).unwrap();
    path_regex
        .find_iter(command)
        .map(|m| m.as_str().to_string())
        .collect()
}

fn extract_urls(command: &str) -> Vec<String> {
    let url_regex = Regex::new(r#"https?://[^\s]+"#).unwrap();
    url_regex
        .find_iter(command)
        .map(|m| m.as_str().to_string())
        .collect()
}

fn extract_package_names(command: &str) -> Vec<String> {
    let parts: Vec<&str> = command.split_whitespace().collect();
    let mut packages = Vec::new();
    
    for (i, part) in parts.iter().enumerate() {
        if *part == "install" && i + 1 < parts.len() {
            packages.push(parts[i + 1].to_string());
        }
    }
    
    packages
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_offline_classification() {
        let classifier = LLMClassifier::new();
        
        let delete_result = classifier.classify_offline("rm -rf /tmp/test.txt");
        assert_eq!(delete_result.intent, Intent::FileDelete);
        assert!(delete_result.risk_score > 0.8);

        let read_result = classifier.classify_offline("cat /etc/passwd");
        assert_eq!(read_result.intent, Intent::FileRead);
        assert!(read_result.risk_score < 0.5);

        let network_result = classifier.classify_offline("curl https://api.example.com");
        assert_eq!(network_result.intent, Intent::NetworkAccess);
        assert!(network_result.risk_score > 0.6);
    }
}
