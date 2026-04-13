use crate::{Tool, ToolContext, ToolError, ToolResult};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};
use tracing::{debug, warn};
use velkor_config::WebSearchConfig;

// ---------------------------------------------------------------------------
// Search result type (provider-agnostic)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct SearchResult {
    title: String,
    url: String,
    snippet: String,
}

impl SearchResult {
    fn to_display(&self) -> String {
        format!("[{}]({})\n{}", self.title, self.url, self.snippet)
    }
}

// ---------------------------------------------------------------------------
// Search provider enum (resolved at startup)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum SearchProvider {
    Tavily { api_key: String },
    Brave { api_key: String },
    Serper { api_key: String },
    Perplexity {
        api_key: String,
        base_url: String,
        model: String,
    },
    DuckDuckGo,
}

impl SearchProvider {
    fn name(&self) -> &'static str {
        match self {
            Self::Tavily { .. } => "Tavily",
            Self::Brave { .. } => "Brave",
            Self::Serper { .. } => "Serper",
            Self::Perplexity { .. } => "Perplexity",
            Self::DuckDuckGo => "DuckDuckGo",
        }
    }
}

// ---------------------------------------------------------------------------
// WebSearchTool
// ---------------------------------------------------------------------------

/// Web search tool with configurable provider.
///
/// Auto mode checks for available API keys in order:
/// Tavily → Brave → Serper → Perplexity → DuckDuckGo (free fallback).
pub struct WebSearchTool {
    client: Client,
    provider: SearchProvider,
    max_results: u32,
}

impl WebSearchTool {
    /// Create from config. Returns `None` if provider is "none".
    pub fn from_config(config: &WebSearchConfig) -> Option<Self> {
        let provider = resolve_provider(config)?;
        debug!(provider = provider.name(), "Web search provider resolved");

        Some(Self {
            client: Client::new(),
            provider,
            max_results: config.max_results,
        })
    }

    /// Create with DuckDuckGo as the default (no config needed).
    pub fn duckduckgo() -> Self {
        Self {
            client: Client::new(),
            provider: SearchProvider::DuckDuckGo,
            max_results: 5,
        }
    }

    async fn search(&self, query: &str) -> Result<Vec<SearchResult>, ToolError> {
        match &self.provider {
            SearchProvider::Tavily { api_key } => {
                search_tavily(&self.client, api_key, query, self.max_results).await
            }
            SearchProvider::Brave { api_key } => {
                search_brave(&self.client, api_key, query, self.max_results).await
            }
            SearchProvider::Serper { api_key } => {
                search_serper(&self.client, api_key, query, self.max_results).await
            }
            SearchProvider::Perplexity {
                api_key,
                base_url,
                model,
            } => search_perplexity(&self.client, api_key, base_url, model, query).await,
            SearchProvider::DuckDuckGo => {
                search_duckduckgo(&self.client, query, self.max_results).await
            }
        }
    }
}

fn resolve_provider(config: &WebSearchConfig) -> Option<SearchProvider> {
    match config.provider.as_str() {
        "none" => None,
        "tavily" => {
            let key = config.tavily_api_key.as_ref()?.clone();
            if key.is_empty() { return None; }
            Some(SearchProvider::Tavily { api_key: key })
        }
        "brave" => {
            let key = config.brave_api_key.as_ref()?.clone();
            if key.is_empty() { return None; }
            Some(SearchProvider::Brave { api_key: key })
        }
        "serper" => {
            let key = config.serper_api_key.as_ref()?.clone();
            if key.is_empty() { return None; }
            Some(SearchProvider::Serper { api_key: key })
        }
        "perplexity" => resolve_perplexity(config),
        "duckduckgo" => Some(SearchProvider::DuckDuckGo),
        "auto" | _ => {
            // Auto: check keys in priority order
            // Tavily → Brave → Serper → Perplexity → DuckDuckGo
            if let Some(ref key) = config.tavily_api_key {
                if !key.is_empty() {
                    return Some(SearchProvider::Tavily {
                        api_key: key.clone(),
                    });
                }
            }
            if let Some(ref key) = config.brave_api_key {
                if !key.is_empty() {
                    return Some(SearchProvider::Brave {
                        api_key: key.clone(),
                    });
                }
            }
            if let Some(ref key) = config.serper_api_key {
                if !key.is_empty() {
                    return Some(SearchProvider::Serper {
                        api_key: key.clone(),
                    });
                }
            }
            if let Some(p) = resolve_perplexity(config) {
                return Some(p);
            }
            Some(SearchProvider::DuckDuckGo)
        }
    }
}

/// Resolve Perplexity provider from config. Auto-detects base URL from API key
/// prefix: `pplx-*` → api.perplexity.ai, `sk-or-*` → openrouter.ai.
fn resolve_perplexity(config: &WebSearchConfig) -> Option<SearchProvider> {
    let pplx = config.perplexity.as_ref()?;
    let api_key = pplx.api_key.as_ref()?.clone();
    if api_key.is_empty() {
        return None;
    }

    // Auto-detect base URL from key prefix if not explicitly set
    let base_url = match &pplx.base_url {
        Some(url) if !url.is_empty() => url.clone(),
        _ => {
            if api_key.starts_with("pplx-") {
                "https://api.perplexity.ai".to_string()
            } else if api_key.starts_with("sk-or-") {
                "https://openrouter.ai/api/v1".to_string()
            } else {
                // Default to Perplexity direct API
                "https://api.perplexity.ai".to_string()
            }
        }
    };

    // Pick the right model name for the endpoint
    let model = match &pplx.model {
        Some(m) if !m.is_empty() => m.clone(),
        _ => {
            if base_url.contains("openrouter") {
                "perplexity/sonar-pro".to_string()
            } else {
                "sonar-pro".to_string()
            }
        }
    };

    Some(SearchProvider::Perplexity {
        api_key,
        base_url,
        model,
    })
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Search the web for current information. Returns titles, URLs, and snippets from search results."
    }

    fn input_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results to return (default: 5)",
                    "minimum": 1,
                    "maximum": 20
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, input: JsonValue, _ctx: &ToolContext) -> Result<ToolResult, ToolError> {
        let query = input
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing 'query' field".into()))?;

        debug!(query, provider = self.provider.name(), "Executing web search");

        let results = self.search(query).await?;

        if results.is_empty() {
            return Ok(ToolResult::success("No results found."));
        }

        let formatted: Vec<String> = results.iter().map(|r| r.to_display()).collect();
        Ok(ToolResult::success(formatted.join("\n\n")))
    }
}

// ---------------------------------------------------------------------------
// Tavily Search API
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct TavilyResponse {
    results: Vec<TavilyResult>,
}

#[derive(Deserialize)]
struct TavilyResult {
    title: String,
    url: String,
    content: String,
}

async fn search_tavily(
    client: &Client,
    api_key: &str,
    query: &str,
    max_results: u32,
) -> Result<Vec<SearchResult>, ToolError> {
    let resp = client
        .post("https://api.tavily.com/search")
        .json(&json!({
            "api_key": api_key,
            "query": query,
            "max_results": max_results,
            "search_depth": "basic"
        }))
        .send()
        .await
        .map_err(|e| ToolError::ExecutionFailed(format!("Tavily request failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(ToolError::ExecutionFailed(format!(
            "Tavily API error ({status}): {body}"
        )));
    }

    let data: TavilyResponse = resp
        .json()
        .await
        .map_err(|e| ToolError::ExecutionFailed(format!("Tavily parse error: {e}")))?;

    Ok(data
        .results
        .into_iter()
        .map(|r| SearchResult {
            title: r.title,
            url: r.url,
            snippet: r.content,
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Brave Search API
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct BraveResponse {
    web: Option<BraveWebResults>,
}

#[derive(Deserialize)]
struct BraveWebResults {
    results: Vec<BraveResult>,
}

#[derive(Deserialize)]
struct BraveResult {
    title: String,
    url: String,
    description: String,
}

async fn search_brave(
    client: &Client,
    api_key: &str,
    query: &str,
    max_results: u32,
) -> Result<Vec<SearchResult>, ToolError> {
    let resp = client
        .get("https://api.search.brave.com/res/v1/web/search")
        .header("X-Subscription-Token", api_key)
        .query(&[("q", query), ("count", &max_results.to_string())])
        .send()
        .await
        .map_err(|e| ToolError::ExecutionFailed(format!("Brave request failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(ToolError::ExecutionFailed(format!(
            "Brave API error ({status}): {body}"
        )));
    }

    let data: BraveResponse = resp
        .json()
        .await
        .map_err(|e| ToolError::ExecutionFailed(format!("Brave parse error: {e}")))?;

    Ok(data
        .web
        .map(|w| {
            w.results
                .into_iter()
                .map(|r| SearchResult {
                    title: r.title,
                    url: r.url,
                    snippet: r.description,
                })
                .collect()
        })
        .unwrap_or_default())
}

// ---------------------------------------------------------------------------
// Serper (Google Search API)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct SerperResponse {
    organic: Option<Vec<SerperResult>>,
}

#[derive(Deserialize)]
struct SerperResult {
    title: String,
    link: String,
    snippet: String,
}

async fn search_serper(
    client: &Client,
    api_key: &str,
    query: &str,
    max_results: u32,
) -> Result<Vec<SearchResult>, ToolError> {
    let resp = client
        .post("https://google.serper.dev/search")
        .header("X-API-KEY", api_key)
        .json(&json!({
            "q": query,
            "num": max_results
        }))
        .send()
        .await
        .map_err(|e| ToolError::ExecutionFailed(format!("Serper request failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(ToolError::ExecutionFailed(format!(
            "Serper API error ({status}): {body}"
        )));
    }

    let data: SerperResponse = resp
        .json()
        .await
        .map_err(|e| ToolError::ExecutionFailed(format!("Serper parse error: {e}")))?;

    Ok(data
        .organic
        .map(|results| {
            results
                .into_iter()
                .map(|r| SearchResult {
                    title: r.title,
                    url: r.link,
                    snippet: r.snippet,
                })
                .collect()
        })
        .unwrap_or_default())
}

// ---------------------------------------------------------------------------
// Perplexity (AI-synthesized search via OpenAI chat completions format)
// ---------------------------------------------------------------------------

/// Perplexity uses the chat completions API to perform search. The query is
/// sent as a user message; the response contains an AI-synthesized answer with
/// citations. Works through OpenRouter (perplexity/sonar-pro) or direct
/// (api.perplexity.ai with model "sonar-pro").
async fn search_perplexity(
    client: &Client,
    api_key: &str,
    base_url: &str,
    model: &str,
    query: &str,
) -> Result<Vec<SearchResult>, ToolError> {
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));

    let body = json!({
        "model": model,
        "messages": [
            {
                "role": "system",
                "content": "You are a search assistant. Provide a concise, factual answer with citations. Include source URLs when available."
            },
            {
                "role": "user",
                "content": query
            }
        ],
        "max_tokens": 1024
    });

    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| ToolError::ExecutionFailed(format!("Perplexity request failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let err_body = resp.text().await.unwrap_or_default();
        return Err(ToolError::ExecutionFailed(format!(
            "Perplexity API error ({status}): {err_body}"
        )));
    }

    let data: PerplexityResponse = resp
        .json()
        .await
        .map_err(|e| ToolError::ExecutionFailed(format!("Perplexity parse error: {e}")))?;

    let answer = data
        .choices
        .into_iter()
        .next()
        .map(|c| c.message.content)
        .unwrap_or_default();

    if answer.is_empty() {
        return Ok(vec![]);
    }

    // Build results: the main synthesized answer + any citations the API returned
    let mut results = Vec::new();

    // Citations come from the top-level `citations` field (Perplexity native API)
    if !data.citations.is_empty() {
        for (i, citation_url) in data.citations.iter().enumerate() {
            results.push(SearchResult {
                title: format!("Source {}", i + 1),
                url: citation_url.clone(),
                snippet: String::new(),
            });
        }
        // Prepend the synthesized answer as the first result
        results.insert(
            0,
            SearchResult {
                title: format!("Perplexity: {query}"),
                url: String::new(),
                snippet: answer,
            },
        );
    } else {
        // No structured citations — return the full answer as one result
        results.push(SearchResult {
            title: format!("Perplexity: {query}"),
            url: String::new(),
            snippet: answer,
        });
    }

    Ok(results)
}

#[derive(Deserialize)]
struct PerplexityResponse {
    choices: Vec<PerplexityChoice>,
    #[serde(default)]
    citations: Vec<String>,
}

#[derive(Deserialize)]
struct PerplexityChoice {
    message: PerplexityMessage,
}

#[derive(Deserialize)]
struct PerplexityMessage {
    #[serde(default)]
    content: String,
}

// ---------------------------------------------------------------------------
// DuckDuckGo (free, no API key)
// ---------------------------------------------------------------------------

/// DuckDuckGo Instant Answer API — free, no auth required.
/// Returns fewer/different results than the paid APIs, but works as a
/// zero-config fallback so the platform is usable out of the box.
async fn search_duckduckgo(
    client: &Client,
    query: &str,
    max_results: u32,
) -> Result<Vec<SearchResult>, ToolError> {
    // DuckDuckGo Instant Answer API
    let resp = client
        .get("https://api.duckduckgo.com/")
        .query(&[("q", query), ("format", "json"), ("no_html", "1")])
        .header("User-Agent", "Velkor/0.1")
        .send()
        .await
        .map_err(|e| ToolError::ExecutionFailed(format!("DuckDuckGo request failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        return Err(ToolError::ExecutionFailed(format!(
            "DuckDuckGo API error ({status})"
        )));
    }

    let data: DdgResponse = resp
        .json()
        .await
        .map_err(|e| ToolError::ExecutionFailed(format!("DuckDuckGo parse error: {e}")))?;

    let mut results = Vec::new();
    let limit = max_results as usize;

    // Abstract (main answer)
    if !data.r#abstract.is_empty() {
        results.push(SearchResult {
            title: data.heading.clone(),
            url: data.abstract_url.clone(),
            snippet: data.r#abstract.clone(),
        });
    }

    // Related topics
    for topic in data.related_topics.iter().take(limit.saturating_sub(results.len())) {
        if let (Some(text), Some(url)) = (&topic.text, &topic.first_url) {
            if !text.is_empty() {
                results.push(SearchResult {
                    title: text.chars().take(80).collect(),
                    url: url.clone(),
                    snippet: text.clone(),
                });
            }
        }
    }

    if results.is_empty() {
        // DDG Instant Answer didn't have structured results — this is
        // expected for many queries. Return a helpful message.
        warn!(query, "DuckDuckGo returned no instant answers — consider configuring Tavily/Brave/Serper for better coverage");
        results.push(SearchResult {
            title: format!("DuckDuckGo search: {query}"),
            url: format!("https://duckduckgo.com/?q={}", urlencoding(query)),
            snippet: "No instant answers available. Try using the web_fetch tool with the DuckDuckGo URL for HTML results, or configure a Tavily/Brave/Serper API key for comprehensive search.".to_string(),
        });
    }

    Ok(results)
}

/// Minimal URL encoding for the DDG fallback URL.
fn urlencoding(s: &str) -> String {
    s.replace(' ', "+")
        .replace('&', "%26")
        .replace('=', "%3D")
}

#[derive(Deserialize)]
struct DdgResponse {
    #[serde(rename = "Abstract", default)]
    r#abstract: String,
    #[serde(rename = "AbstractURL", default)]
    abstract_url: String,
    #[serde(rename = "Heading", default)]
    heading: String,
    #[serde(rename = "RelatedTopics", default)]
    related_topics: Vec<DdgTopic>,
}

#[derive(Deserialize)]
struct DdgTopic {
    #[serde(rename = "Text")]
    text: Option<String>,
    #[serde(rename = "FirstURL")]
    first_url: Option<String>,
}
