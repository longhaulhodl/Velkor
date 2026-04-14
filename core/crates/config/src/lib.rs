use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("failed to read config file: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse YAML: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("missing required environment variable: {0}")]
    MissingEnvVar(String),
}

// ---------------------------------------------------------------------------
// Top-level platform config (maps to config.yaml)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct PlatformConfig {
    pub platform: PlatformSection,
    pub database: DatabaseSection,
    #[serde(default)]
    pub providers: HashMap<String, ProviderConfig>,
    #[serde(default)]
    pub routing: Option<RoutingSection>,
    #[serde(default)]
    pub memory: Option<MemorySection>,
    #[serde(default)]
    pub scheduling: Option<SchedulingSection>,
    #[serde(default)]
    pub retention: Option<RetentionSection>,
    #[serde(default)]
    pub channels: Option<ChannelsSection>,
    #[serde(default)]
    pub tools: Option<ToolsConfig>,
}

// ---------------------------------------------------------------------------
// Tools configuration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ToolsConfig {
    #[serde(default)]
    pub web_search: Option<WebSearchConfig>,
    #[serde(default)]
    pub web_fetch: Option<WebFetchConfig>,
    #[serde(default)]
    pub memory: Option<ToolToggle>,
    #[serde(default)]
    pub documents: Option<ToolToggle>,
    #[serde(default)]
    pub skills: Option<SkillsConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SkillsConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Directories to scan for installable SKILL.md files.
    #[serde(default)]
    pub directories: Vec<String>,
    /// Enable background post-turn skill review (Hermes self-improvement pattern).
    #[serde(default)]
    pub self_improve: bool,
    /// Minimum ReAct iterations before triggering skill review.
    #[serde(default = "default_review_threshold")]
    pub review_threshold: u32,
}

impl Default for SkillsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            directories: vec!["skills".into()],
            self_improve: true,
            review_threshold: 10,
        }
    }
}

fn default_review_threshold() -> u32 {
    10
}

#[derive(Debug, Clone, Deserialize)]
pub struct WebSearchConfig {
    /// "auto" | "tavily" | "brave" | "serper" | "perplexity" | "duckduckgo" | "none"
    #[serde(default = "default_web_search_provider")]
    pub provider: String,
    pub tavily_api_key: Option<String>,
    pub brave_api_key: Option<String>,
    pub serper_api_key: Option<String>,
    #[serde(default)]
    pub perplexity: Option<PerplexityConfig>,
    #[serde(default = "default_max_results")]
    pub max_results: u32,
}

impl Default for WebSearchConfig {
    fn default() -> Self {
        Self {
            provider: default_web_search_provider(),
            tavily_api_key: None,
            brave_api_key: None,
            serper_api_key: None,
            perplexity: None,
            max_results: default_max_results(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct PerplexityConfig {
    pub api_key: Option<String>,
    /// Overrides auto-detected base URL. Auto-detect: pplx-* → api.perplexity.ai,
    /// sk-or-* → openrouter.ai/api/v1.
    pub base_url: Option<String>,
    /// Model to use. Default: "perplexity/sonar-pro" (OpenRouter) or "sonar-pro" (direct).
    pub model: Option<String>,
}

fn default_web_search_provider() -> String {
    "auto".into()
}

fn default_max_results() -> u32 {
    5
}

#[derive(Debug, Clone, Deserialize)]
pub struct WebFetchConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_max_content_length")]
    pub max_content_length: usize,
}

impl Default for WebFetchConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_content_length: default_max_content_length(),
        }
    }
}

fn default_max_content_length() -> usize {
    50_000
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolToggle {
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlatformSection {
    pub name: String,
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub secret_key: String,
}

fn default_host() -> String {
    "0.0.0.0".into()
}

fn default_port() -> u16 {
    8080
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseSection {
    pub postgres_url: String,
    #[serde(default = "default_redis_url")]
    pub redis_url: String,
    #[serde(default)]
    pub s3: Option<S3Config>,
}

fn default_redis_url() -> String {
    "redis://localhost:6379".into()
}

#[derive(Debug, Clone, Deserialize)]
pub struct S3Config {
    pub endpoint: String,
    pub access_key: String,
    pub secret_key: String,
    #[serde(default = "default_bucket")]
    pub bucket: String,
}

fn default_bucket() -> String {
    "agentplatform".into()
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProviderConfig {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub default_model: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RoutingSection {
    #[serde(default = "default_strategy")]
    pub strategy: String,
    #[serde(default)]
    pub fallback_chain: Vec<String>,
    pub cost_limit_daily_usd: Option<f64>,
    pub cost_limit_monthly_usd: Option<f64>,
}

fn default_strategy() -> String {
    "cost_optimized".into()
}

#[derive(Debug, Clone, Deserialize)]
pub struct MemorySection {
    #[serde(default = "default_embedding_model")]
    pub embedding_model: String,
    #[serde(default = "default_embedding_dims")]
    pub embedding_dimensions: u32,
    #[serde(default = "default_true")]
    pub auto_memorize: bool,
    #[serde(default = "default_true")]
    pub user_modeling: bool,
    #[serde(default)]
    pub backend: Option<String>,
    #[serde(default)]
    pub context_compression: Option<ContextCompressionConfig>,
}

fn default_embedding_model() -> String {
    "openai/text-embedding-3-small".into()
}

fn default_embedding_dims() -> u32 {
    1536
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContextCompressionConfig {
    #[serde(default = "default_compression_strategy")]
    pub strategy: String,
    #[serde(default = "default_max_context")]
    pub max_context_tokens: u64,
}

fn default_compression_strategy() -> String {
    "summarize".into()
}

fn default_max_context() -> u64 {
    100_000
}

#[derive(Debug, Clone, Deserialize)]
pub struct SchedulingSection {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_heartbeat")]
    pub heartbeat_interval_seconds: u64,
    pub timezone: Option<String>,
}

fn default_heartbeat() -> u64 {
    60
}

#[derive(Debug, Clone, Deserialize)]
pub struct RetentionSection {
    pub default_conversation_days: Option<u32>,
    pub default_memory_days: Option<u32>,
    pub default_document_days: Option<u32>,
    pub audit_log_days: Option<u32>,
    #[serde(default = "default_true")]
    pub auto_purge: bool,
    pub purge_schedule: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChannelsSection {
    #[serde(default)]
    pub web: Option<ChannelToggle>,
    #[serde(default)]
    pub api: Option<ApiChannelConfig>,
    #[serde(default)]
    pub telegram: Option<BotChannelConfig>,
    #[serde(default)]
    pub discord: Option<BotChannelConfig>,
    #[serde(default)]
    pub slack: Option<SlackChannelConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChannelToggle {
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiChannelConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub rate_limit_per_minute: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BotChannelConfig {
    #[serde(default)]
    pub enabled: bool,
    pub bot_token: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SlackChannelConfig {
    #[serde(default)]
    pub enabled: bool,
    pub bot_token: Option<String>,
    pub app_token: Option<String>,
}

// ---------------------------------------------------------------------------
// Agent definition config (agents/*.yaml)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct AgentConfig {
    pub agent: AgentDefinition,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentDefinition {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub model: String,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub skills: Vec<String>,
    pub max_context_tokens: Option<u64>,
    pub max_tool_calls_per_turn: Option<u32>,
    #[serde(default)]
    pub self_improve: bool,
    pub memory_scope: Option<String>,
}

fn default_temperature() -> f32 {
    0.7
}

// ---------------------------------------------------------------------------
// Environment variable substitution
// ---------------------------------------------------------------------------

/// Substitute `${VAR}` and `${VAR:-default}` patterns in a string using
/// environment variables. Returns an error if a variable without a default
/// is missing from the environment.
pub fn substitute_env_vars(input: &str) -> Result<String, ConfigError> {
    let re = Regex::new(r"\$\{([^}]+)\}").expect("valid regex");
    let mut result = input.to_string();
    let mut missing: Option<String> = None;

    // Iterate in reverse so replacement ranges stay valid
    let captures: Vec<_> = re.captures_iter(input).collect();
    for cap in captures.iter().rev() {
        let full_match = cap.get(0).unwrap();
        let expr = &cap[1];

        let (var_name, default_val) = if let Some(idx) = expr.find(":-") {
            (&expr[..idx], Some(&expr[idx + 2..]))
        } else {
            (expr, None)
        };

        let value = match std::env::var(var_name) {
            Ok(v) => v,
            Err(_) => match default_val {
                Some(d) => d.to_string(),
                None => {
                    missing = Some(var_name.to_string());
                    continue;
                }
            },
        };

        result.replace_range(full_match.range(), &value);
    }

    if let Some(var) = missing {
        return Err(ConfigError::MissingEnvVar(var));
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

/// Load and parse the main platform config from a YAML file, performing
/// environment variable substitution before deserialization.
pub fn load_platform_config(path: &Path) -> Result<PlatformConfig, ConfigError> {
    let raw = std::fs::read_to_string(path)?;
    let substituted = substitute_env_vars(&raw)?;
    let config: PlatformConfig = serde_yaml::from_str(&substituted)?;
    Ok(config)
}

/// Load an agent definition from a YAML file.
pub fn load_agent_config(path: &Path) -> Result<AgentConfig, ConfigError> {
    let raw = std::fs::read_to_string(path)?;
    let substituted = substitute_env_vars(&raw)?;
    let config: AgentConfig = serde_yaml::from_str(&substituted)?;
    Ok(config)
}

/// Load all agent configs from a directory.
pub fn load_all_agents(dir: &Path) -> Result<Vec<AgentConfig>, ConfigError> {
    let mut agents = Vec::new();
    if dir.is_dir() {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("yaml") {
                agents.push(load_agent_config(&path)?);
            }
        }
    }
    Ok(agents)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// SAFETY: these tests mutate process-wide env vars. Run with
    /// `cargo test -- --test-threads=1` if parallel test issues arise.
    /// In Rust 2024, set_var/remove_var are unsafe due to thread safety.
    unsafe fn set_env(key: &str, val: &str) {
        unsafe { std::env::set_var(key, val) };
    }

    unsafe fn remove_env(key: &str) {
        unsafe { std::env::remove_var(key) };
    }

    #[test]
    fn test_env_var_substitution_with_default() {
        unsafe { remove_env("_TEST_MISSING_VAR") };
        let input = "host: ${_TEST_MISSING_VAR:-localhost}";
        let result = substitute_env_vars(input).unwrap();
        assert_eq!(result, "host: localhost");
    }

    #[test]
    fn test_env_var_substitution_with_value() {
        unsafe { set_env("_TEST_PRESENT_VAR", "myvalue") };
        let input = "key: ${_TEST_PRESENT_VAR}";
        let result = substitute_env_vars(input).unwrap();
        assert_eq!(result, "key: myvalue");
        unsafe { remove_env("_TEST_PRESENT_VAR") };
    }

    #[test]
    fn test_env_var_substitution_override_default() {
        unsafe { set_env("_TEST_OVERRIDE", "override_val") };
        let input = "key: ${_TEST_OVERRIDE:-fallback}";
        let result = substitute_env_vars(input).unwrap();
        assert_eq!(result, "key: override_val");
        unsafe { remove_env("_TEST_OVERRIDE") };
    }

    #[test]
    fn test_env_var_missing_no_default_errors() {
        unsafe { remove_env("_TEST_REQUIRED_MISSING") };
        let input = "secret: ${_TEST_REQUIRED_MISSING}";
        let result = substitute_env_vars(input);
        assert!(result.is_err());
        match result.unwrap_err() {
            ConfigError::MissingEnvVar(name) => assert_eq!(name, "_TEST_REQUIRED_MISSING"),
            other => panic!("expected MissingEnvVar, got: {other}"),
        }
    }

    #[test]
    fn test_multiple_substitutions() {
        unsafe {
            set_env("_TEST_A", "alpha");
            set_env("_TEST_B", "beta");
        }
        let input = "${_TEST_A} and ${_TEST_B} and ${_TEST_C:-gamma}";
        let result = substitute_env_vars(input).unwrap();
        assert_eq!(result, "alpha and beta and gamma");
        unsafe {
            remove_env("_TEST_A");
            remove_env("_TEST_B");
        }
    }

    #[test]
    fn test_no_substitution_needed() {
        let input = "plain text without any variables";
        let result = substitute_env_vars(input).unwrap();
        assert_eq!(result, input);
    }

    #[test]
    fn test_parse_minimal_config() {
        unsafe { set_env("_TEST_SECRET", "s3cret") };
        let yaml = r#"
platform:
  name: "Test Platform"
  secret_key: "${_TEST_SECRET}"
database:
  postgres_url: "${_TEST_DB:-postgresql://localhost/test}"
"#;
        let substituted = substitute_env_vars(yaml).unwrap();
        let config: PlatformConfig = serde_yaml::from_str(&substituted).unwrap();
        assert_eq!(config.platform.name, "Test Platform");
        assert_eq!(config.platform.secret_key, "s3cret");
        assert_eq!(config.database.postgres_url, "postgresql://localhost/test");
        assert_eq!(config.platform.port, 8080); // default
        unsafe { remove_env("_TEST_SECRET") };
    }
}
