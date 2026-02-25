use crate::llm::{Classification, Intent};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::fs;
use regex::Regex;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PolicyError {
    #[error("Policy file not found: {0}")]
    NotFound(String),
    #[error("Failed to parse policy: {0}")]
    ParseError(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Action {
    Allow,
    Deny,
    RequireApproval,
    Log,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRule {
    pub name: String,
    pub priority: i32,
    pub conditions: RuleConditions,
    pub action: Action,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleConditions {
    #[serde(default)]
    pub intents: Vec<String>,
    #[serde(default)]
    pub path_patterns: Vec<String>,
    #[serde(default)]
    pub command_patterns: Vec<String>,
    #[serde(default)]
    pub min_risk_score: Option<f64>,
    #[serde(default)]
    pub max_risk_score: Option<f64>,
    #[serde(default)]
    pub min_confidence: Option<f64>,
    #[serde(default)]
    pub business_hours_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimits {
    pub operations_per_minute: usize,
    pub deletes_per_hour: usize,
    pub network_requests_per_hour: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskThresholds {
    pub auto_approve: f64,
    pub require_approval: f64,
    pub auto_deny: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyConfig {
    pub risk_thresholds: RiskThresholds,
    pub rate_limits: RateLimits,
    pub rules: Vec<PolicyRule>,
}

pub struct PolicyEngine {
    config: PolicyConfig,
    operation_count: std::sync::Arc<std::sync::Mutex<OperationCounter>>,
}

#[derive(Debug, Default)]
struct OperationCounter {
    last_minute: std::collections::VecDeque<std::time::Instant>,
    last_hour_deletes: std::collections::VecDeque<std::time::Instant>,
    last_hour_network: std::collections::VecDeque<std::time::Instant>,
}

impl PolicyEngine {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, PolicyError> {
        let contents = fs::read_to_string(path.as_ref())
            .map_err(|e| PolicyError::NotFound(format!("{}: {}", path.as_ref().display(), e)))?;
        
        let config: PolicyConfig = serde_yaml::from_str(&contents)
            .map_err(|e| PolicyError::ParseError(e.to_string()))?;

        Ok(Self {
            config,
            operation_count: std::sync::Arc::new(std::sync::Mutex::new(OperationCounter::default())),
        })
    }

    pub fn new(config: PolicyConfig) -> Self {
        Self {
            config,
            operation_count: std::sync::Arc::new(std::sync::Mutex::new(OperationCounter::default())),
        }
    }

    pub fn evaluate(&self, classification: &Classification, command: &str) -> Action {
        // Check rate limits first
        if !self.check_rate_limits(classification) {
            tracing::warn!("Rate limit exceeded for command: {}", command);
            return Action::Deny;
        }

        // Apply risk thresholds
        if classification.risk_score >= self.config.risk_thresholds.auto_deny {
            tracing::warn!(
                "Command auto-denied due to high risk score: {} (threshold: {})",
                classification.risk_score,
                self.config.risk_thresholds.auto_deny
            );
            return Action::Deny;
        }

        // Evaluate rules (sorted by priority)
        let mut rules = self.config.rules.clone();
        rules.sort_by(|a, b| b.priority.cmp(&a.priority));

        for rule in rules {
            if self.matches_rule(&rule, classification, command) {
                tracing::info!(
                    "Command matched rule '{}' with action: {:?}",
                    rule.name,
                    rule.action
                );
                return rule.action;
            }
        }

        // Default based on risk threshold
        if classification.risk_score >= self.config.risk_thresholds.require_approval {
            tracing::info!(
                "Command requires approval due to risk score: {} (threshold: {})",
                classification.risk_score,
                self.config.risk_thresholds.require_approval
            );
            Action::RequireApproval
        } else if classification.risk_score <= self.config.risk_thresholds.auto_approve {
            tracing::info!(
                "Command auto-approved due to low risk score: {} (threshold: {})",
                classification.risk_score,
                self.config.risk_thresholds.auto_approve
            );
            Action::Allow
        } else {
            Action::RequireApproval
        }
    }

    fn matches_rule(&self, rule: &PolicyRule, classification: &Classification, command: &str) -> bool {
        let conditions = &rule.conditions;

        // Check intent match
        if !conditions.intents.is_empty() {
            let intent_str = format!("{:?}", classification.intent);
            if !conditions.intents.iter().any(|i| i == &intent_str) {
                return false;
            }
        }

        // Check risk score range
        if let Some(min_risk) = conditions.min_risk_score {
            if classification.risk_score < min_risk {
                return false;
            }
        }
        if let Some(max_risk) = conditions.max_risk_score {
            if classification.risk_score > max_risk {
                return false;
            }
        }

        // Check confidence
        if let Some(min_confidence) = conditions.min_confidence {
            if classification.confidence < min_confidence {
                return false;
            }
        }

        // Check path patterns
        if !conditions.path_patterns.is_empty() {
            let mut path_matched = false;
            for resource in &classification.affected_resources {
                for pattern in &conditions.path_patterns {
                    if let Ok(regex) = Regex::new(pattern) {
                        if regex.is_match(resource) {
                            path_matched = true;
                            break;
                        }
                    }
                }
                if path_matched {
                    break;
                }
            }
            if !path_matched && !classification.affected_resources.is_empty() {
                return false;
            }
        }

        // Check command patterns
        if !conditions.command_patterns.is_empty() {
            let mut command_matched = false;
            for pattern in &conditions.command_patterns {
                if let Ok(regex) = Regex::new(pattern) {
                    if regex.is_match(command) {
                        command_matched = true;
                        break;
                    }
                }
            }
            if !command_matched {
                return false;
            }
        }

        // Check business hours
        if conditions.business_hours_only {
            if !is_business_hours() {
                return false;
            }
        }

        true
    }

    fn check_rate_limits(&self, classification: &Classification) -> bool {
        let mut counter = self.operation_count.lock().unwrap();
        let now = std::time::Instant::now();
        let one_minute_ago = now - std::time::Duration::from_secs(60);
        let one_hour_ago = now - std::time::Duration::from_secs(3600);

        // Clean old entries
        counter.last_minute.retain(|&t| t > one_minute_ago);
        counter.last_hour_deletes.retain(|&t| t > one_hour_ago);
        counter.last_hour_network.retain(|&t| t > one_hour_ago);

        // Check operations per minute
        if counter.last_minute.len() >= self.config.rate_limits.operations_per_minute {
            return false;
        }

        // Check delete operations per hour
        if classification.intent == Intent::FileDelete {
            if counter.last_hour_deletes.len() >= self.config.rate_limits.deletes_per_hour {
                return false;
            }
        }

        // Check network operations per hour
        if classification.intent == Intent::NetworkAccess {
            if counter.last_hour_network.len() >= self.config.rate_limits.network_requests_per_hour {
                return false;
            }
        }

        // Record this operation
        counter.last_minute.push_back(now);
        if classification.intent == Intent::FileDelete {
            counter.last_hour_deletes.push_back(now);
        }
        if classification.intent == Intent::NetworkAccess {
            counter.last_hour_network.push_back(now);
        }

        true
    }

    pub fn get_config(&self) -> &PolicyConfig {
        &self.config
    }
}

fn is_business_hours() -> bool {
    use chrono::{Datelike, Timelike, Utc};
    
    let now = Utc::now();
    let weekday = now.weekday();
    let hour = now.hour();

    // Monday = 0, Friday = 4
    let is_weekday = weekday.num_days_from_monday() < 5;
    let is_work_hours = hour >= 9 && hour < 17;

    is_weekday && is_work_hours
}

impl Default for PolicyConfig {
    fn default() -> Self {
        PolicyConfig {
            risk_thresholds: RiskThresholds {
                auto_approve: 0.3,
                require_approval: 0.6,
                auto_deny: 0.9,
            },
            rate_limits: RateLimits {
                operations_per_minute: 60,
                deletes_per_hour: 10,
                network_requests_per_hour: 100,
            },
            rules: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_policy_evaluation() {
        let config = PolicyConfig::default();
        let engine = PolicyEngine::new(config);

        let low_risk = Classification {
            intent: Intent::FileRead,
            risk_score: 0.2,
            confidence: 0.9,
            affected_resources: vec!["test.txt".to_string()],
            reasoning: "Test".to_string(),
        };

        assert_eq!(engine.evaluate(&low_risk, "cat test.txt"), Action::Allow);

        let high_risk = Classification {
            intent: Intent::FileDelete,
            risk_score: 0.95,
            confidence: 0.9,
            affected_resources: vec!["/important/data".to_string()],
            reasoning: "Test".to_string(),
        };

        assert_eq!(engine.evaluate(&high_risk, "rm -rf /important/data"), Action::Deny);
    }

    #[test]
    fn test_rule_matching() {
        let rule = PolicyRule {
            name: "Test Rule".to_string(),
            priority: 10,
            conditions: RuleConditions {
                intents: vec!["FileWrite".to_string()],
                path_patterns: vec![r"\.txt$".to_string()],
                command_patterns: vec![],
                min_risk_score: Some(0.3),
                max_risk_score: Some(0.7),
                min_confidence: None,
                business_hours_only: false,
            },
            action: Action::RequireApproval,
        };

        let config = PolicyConfig {
            risk_thresholds: RiskThresholds::default(),
            rate_limits: RateLimits::default(),
            rules: vec![rule],
        };

        let engine = PolicyEngine::new(config);

        let classification = Classification {
            intent: Intent::FileWrite,
            risk_score: 0.5,
            confidence: 0.8,
            affected_resources: vec!["test.txt".to_string()],
            reasoning: "Test".to_string(),
        };

        assert_eq!(engine.evaluate(&classification, "write test.txt"), Action::RequireApproval);
    }
}

impl Default for RiskThresholds {
    fn default() -> Self {
        RiskThresholds {
            auto_approve: 0.3,
            require_approval: 0.6,
            auto_deny: 0.9,
        }
    }
}

impl Default for RateLimits {
    fn default() -> Self {
        RateLimits {
            operations_per_minute: 60,
            deletes_per_hour: 10,
            network_requests_per_hour: 100,
        }
    }
}
