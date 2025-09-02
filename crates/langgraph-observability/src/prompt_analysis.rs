//! Prompt analysis and debugging features

use crate::error::{ObservabilityError, ObservabilityResult};
use crate::storage::{PromptExecution, TokenUsage};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Prompt analyzer for debugging and optimization
pub struct PromptAnalyzer {
    config: PromptAnalysisConfig,
}

/// Configuration for prompt analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptAnalysisConfig {
    /// Whether to analyze token efficiency
    pub analyze_token_efficiency: bool,
    /// Whether to detect potential prompt injection
    pub detect_injection: bool,
    /// Whether to analyze prompt patterns
    pub analyze_patterns: bool,
    /// Maximum prompt length to analyze
    pub max_prompt_length: usize,
}

impl Default for PromptAnalysisConfig {
    fn default() -> Self {
        Self {
            analyze_token_efficiency: true,
            detect_injection: true,
            analyze_patterns: true,
            max_prompt_length: 10000,
        }
    }
}

impl PromptAnalyzer {
    /// Create a new prompt analyzer
    pub fn new(config: PromptAnalysisConfig) -> Self {
        Self { config }
    }

    /// Analyze a prompt execution
    pub async fn analyze_prompt(&self, prompt: &PromptExecution) -> ObservabilityResult<PromptAnalysis> {
        let mut analysis = PromptAnalysis {
            prompt_id: prompt.id.clone(),
            run_id: prompt.run_id.clone(),
            model: prompt.model.clone(),
            input_length: prompt.input.len(),
            output_length: prompt.output.len(),
            token_efficiency: None,
            injection_risk: None,
            patterns: vec![],
            suggestions: vec![],
            quality_score: 0.0,
        };

        // Analyze token efficiency
        if self.config.analyze_token_efficiency {
            analysis.token_efficiency = Some(self.analyze_token_efficiency(prompt).await?);
        }

        // Detect injection risks
        if self.config.detect_injection {
            analysis.injection_risk = Some(self.detect_injection_risk(prompt).await?);
        }

        // Analyze patterns
        if self.config.analyze_patterns {
            analysis.patterns = self.analyze_patterns(prompt).await?;
        }

        // Generate suggestions
        analysis.suggestions = self.generate_suggestions(&analysis).await?;

        // Calculate quality score
        analysis.quality_score = self.calculate_quality_score(&analysis).await?;

        Ok(analysis)
    }

    /// Analyze token efficiency
    async fn analyze_token_efficiency(&self, prompt: &PromptExecution) -> ObservabilityResult<TokenEfficiency> {
        let input_ratio = prompt.token_usage.input_tokens as f64 / prompt.input.len() as f64;
        let output_ratio = prompt.token_usage.output_tokens as f64 / prompt.output.len() as f64;
        
        let efficiency_score = if prompt.token_usage.total_tokens > 0 {
            let useful_output_ratio = prompt.output.len() as f64 / prompt.token_usage.total_tokens as f64;
            useful_output_ratio * 100.0
        } else {
            0.0
        };

        Ok(TokenEfficiency {
            input_char_to_token_ratio: input_ratio,
            output_char_to_token_ratio: output_ratio,
            efficiency_score,
            total_tokens: prompt.token_usage.total_tokens,
            cost_estimate: self.estimate_cost(&prompt.model, &prompt.token_usage).await?,
        })
    }

    /// Detect potential prompt injection
    async fn detect_injection_risk(&self, prompt: &PromptExecution) -> ObservabilityResult<InjectionRisk> {
        let mut risk_factors = vec![];
        let mut risk_score = 0.0;

        // Check for common injection patterns
        let injection_patterns = [
            "ignore previous instructions",
            "system:",
            "\\n\\nHuman:",
            "\\n\\nAssistant:",
            "forget everything",
            "you are now",
            "roleplay",
            "pretend",
        ];

        for pattern in &injection_patterns {
            if prompt.input.to_lowercase().contains(pattern) {
                risk_factors.push(format!("Contains suspicious pattern: {}", pattern));
                risk_score += 10.0;
            }
        }

        // Check for excessive system-like instructions
        if prompt.input.matches("system").count() > 2 {
            risk_factors.push("Multiple system references detected".to_string());
            risk_score += 15.0;
        }

        // Check for unusual formatting
        if prompt.input.matches("\\n").count() > 10 {
            risk_factors.push("Excessive line breaks detected".to_string());
            risk_score += 5.0;
        }

        let risk_level = if risk_score >= 30.0 {
            RiskLevel::High
        } else if risk_score >= 15.0 {
            RiskLevel::Medium
        } else if risk_score > 0.0 {
            RiskLevel::Low
        } else {
            RiskLevel::None
        };

        Ok(InjectionRisk {
            risk_level,
            risk_score,
            risk_factors,
        })
    }

    /// Analyze prompt patterns
    async fn analyze_patterns(&self, prompt: &PromptExecution) -> ObservabilityResult<Vec<PromptPattern>> {
        let mut patterns = vec![];

        // Check for few-shot examples
        if prompt.input.matches("Example:").count() >= 2 {
            patterns.push(PromptPattern {
                pattern_type: PatternType::FewShot,
                confidence: 0.8,
                description: "Multiple examples detected in prompt".to_string(),
            });
        }

        // Check for chain-of-thought prompting
        if prompt.input.to_lowercase().contains("think step by step") ||
           prompt.input.to_lowercase().contains("let's think") {
            patterns.push(PromptPattern {
                pattern_type: PatternType::ChainOfThought,
                confidence: 0.9,
                description: "Chain-of-thought prompting detected".to_string(),
            });
        }

        // Check for role-playing
        if prompt.input.to_lowercase().contains("you are a") ||
           prompt.input.to_lowercase().contains("act as") {
            patterns.push(PromptPattern {
                pattern_type: PatternType::RolePlay,
                confidence: 0.7,
                description: "Role-playing prompt detected".to_string(),
            });
        }

        // Check for structured output requests
        if prompt.input.to_lowercase().contains("json") ||
           prompt.input.to_lowercase().contains("format:") ||
           prompt.input.to_lowercase().contains("output format") {
            patterns.push(PromptPattern {
                pattern_type: PatternType::StructuredOutput,
                confidence: 0.8,
                description: "Structured output request detected".to_string(),
            });
        }

        Ok(patterns)
    }

    /// Generate improvement suggestions
    async fn generate_suggestions(&self, analysis: &PromptAnalysis) -> ObservabilityResult<Vec<String>> {
        let mut suggestions = vec![];

        // Token efficiency suggestions
        if let Some(ref efficiency) = analysis.token_efficiency {
            if efficiency.efficiency_score < 50.0 {
                suggestions.push("Consider making the prompt more concise to improve token efficiency".to_string());
            }

            if efficiency.total_tokens > 2000 {
                suggestions.push("Prompt is quite long - consider breaking it into smaller parts".to_string());
            }
        }

        // Injection risk suggestions
        if let Some(ref risk) = analysis.injection_risk {
            match risk.risk_level {
                RiskLevel::High => {
                    suggestions.push("HIGH RISK: Review prompt for potential injection attempts".to_string());
                }
                RiskLevel::Medium => {
                    suggestions.push("Medium risk: Consider adding input validation".to_string());
                }
                _ => {}
            }
        }

        // Pattern-based suggestions
        for pattern in &analysis.patterns {
            match pattern.pattern_type {
                PatternType::FewShot => {
                    suggestions.push("Good use of few-shot examples for better performance".to_string());
                }
                PatternType::ChainOfThought => {
                    suggestions.push("Chain-of-thought prompting can improve reasoning - good choice".to_string());
                }
                PatternType::StructuredOutput => {
                    suggestions.push("Consider using JSON schema validation for structured outputs".to_string());
                }
                _ => {}
            }
        }

        Ok(suggestions)
    }

    /// Calculate overall quality score
    async fn calculate_quality_score(&self, analysis: &PromptAnalysis) -> ObservabilityResult<f64> {
        let mut score = 50.0; // Base score

        // Token efficiency impact
        if let Some(ref efficiency) = analysis.token_efficiency {
            score += efficiency.efficiency_score * 0.3;
        }

        // Injection risk impact (negative)
        if let Some(ref risk) = analysis.injection_risk {
            score -= risk.risk_score * 0.5;
        }

        // Pattern bonus
        for pattern in &analysis.patterns {
            match pattern.pattern_type {
                PatternType::FewShot | PatternType::ChainOfThought => {
                    score += 10.0 * pattern.confidence;
                }
                _ => {
                    score += 5.0 * pattern.confidence;
                }
            }
        }

        // Ensure score is within bounds
        Ok(score.max(0.0).min(100.0))
    }

    /// Estimate cost based on token usage
    async fn estimate_cost(&self, model: &str, token_usage: &TokenUsage) -> ObservabilityResult<f64> {
        // Simplified cost estimation - in practice, you'd have a database of model pricing
        let cost_per_1k_tokens = match model {
            model if model.contains("gpt-4") => 0.03,
            model if model.contains("gpt-3.5") => 0.002,
            model if model.contains("claude") => 0.008,
            _ => 0.001, // Default/unknown model
        };

        let total_cost = (token_usage.total_tokens as f64 / 1000.0) * cost_per_1k_tokens;
        Ok(total_cost)
    }

    /// Analyze multiple prompts to find patterns across a run
    pub async fn analyze_run_prompts(&self, prompts: &[PromptExecution]) -> ObservabilityResult<RunPromptAnalysis> {
        let mut analyses = vec![];
        for prompt in prompts {
            analyses.push(self.analyze_prompt(prompt).await?);
        }

        let total_tokens: u32 = prompts.iter().map(|p| p.token_usage.total_tokens).sum();
        let total_cost: f64 = analyses
            .iter()
            .filter_map(|a| a.token_efficiency.as_ref().map(|e| e.cost_estimate))
            .sum();

        let avg_quality: f64 = analyses.iter().map(|a| a.quality_score).sum::<f64>() / analyses.len() as f64;

        // Find common patterns
        let mut pattern_counts = HashMap::new();
        for analysis in &analyses {
            for pattern in &analysis.patterns {
                *pattern_counts.entry(pattern.pattern_type.clone()).or_insert(0) += 1;
            }
        }

        Ok(RunPromptAnalysis {
            total_prompts: prompts.len(),
            total_tokens,
            total_cost,
            average_quality_score: avg_quality,
            individual_analyses: analyses,
            common_patterns: pattern_counts,
        })
    }
}

/// Complete analysis of a prompt execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptAnalysis {
    pub prompt_id: String,
    pub run_id: String,
    pub model: String,
    pub input_length: usize,
    pub output_length: usize,
    pub token_efficiency: Option<TokenEfficiency>,
    pub injection_risk: Option<InjectionRisk>,
    pub patterns: Vec<PromptPattern>,
    pub suggestions: Vec<String>,
    pub quality_score: f64,
}

/// Token efficiency analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenEfficiency {
    pub input_char_to_token_ratio: f64,
    pub output_char_to_token_ratio: f64,
    pub efficiency_score: f64,
    pub total_tokens: u32,
    pub cost_estimate: f64,
}

/// Injection risk analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InjectionRisk {
    pub risk_level: RiskLevel,
    pub risk_score: f64,
    pub risk_factors: Vec<String>,
}

/// Risk levels for prompt injection
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RiskLevel {
    None,
    Low,
    Medium,
    High,
}

/// Detected prompt patterns
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptPattern {
    pub pattern_type: PatternType,
    pub confidence: f64,
    pub description: String,
}

/// Types of prompt patterns
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum PatternType {
    FewShot,
    ChainOfThought,
    RolePlay,
    StructuredOutput,
    Question,
    Instruction,
    Conversation,
}

/// Analysis of all prompts in a run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunPromptAnalysis {
    pub total_prompts: usize,
    pub total_tokens: u32,
    pub total_cost: f64,
    pub average_quality_score: f64,
    pub individual_analyses: Vec<PromptAnalysis>,
    pub common_patterns: HashMap<PatternType, u32>,
}

/// Prompt debugging session
pub struct PromptDebugSession {
    pub session_id: String,
    pub original_prompt: String,
    pub variations: Vec<PromptVariation>,
    pub metrics: DebugMetrics,
}

/// A variation of a prompt for A/B testing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptVariation {
    pub id: String,
    pub prompt: String,
    pub results: Vec<PromptExecution>,
    pub analysis: Option<PromptAnalysis>,
}

/// Metrics for prompt debugging
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugMetrics {
    pub response_quality: f64,
    pub token_efficiency: f64,
    pub latency: f64,
    pub cost: f64,
    pub success_rate: f64,
}

impl PromptDebugSession {
    /// Create a new debugging session
    pub fn new(session_id: String, original_prompt: String) -> Self {
        Self {
            session_id,
            original_prompt,
            variations: vec![],
            metrics: DebugMetrics {
                response_quality: 0.0,
                token_efficiency: 0.0,
                latency: 0.0,
                cost: 0.0,
                success_rate: 0.0,
            },
        }
    }

    /// Add a prompt variation to test
    pub fn add_variation(&mut self, variation: PromptVariation) {
        self.variations.push(variation);
    }

    /// Get the best performing variation
    pub fn get_best_variation(&self) -> Option<&PromptVariation> {
        self.variations
            .iter()
            .max_by(|a, b| {
                let score_a = a.analysis.as_ref().map(|a| a.quality_score).unwrap_or(0.0);
                let score_b = b.analysis.as_ref().map(|a| a.quality_score).unwrap_or(0.0);
                score_a.partial_cmp(&score_b).unwrap()
            })
    }
}
