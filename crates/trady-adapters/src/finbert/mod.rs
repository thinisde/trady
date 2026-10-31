//! Explicitly enabled, bounded HTTP client for the self-hosted FinBERT sidecar.

use std::{collections::BTreeSet, io::Read, time::Duration};

use reqwest::{Url, blocking::Client, redirect::Policy};
use serde::{Deserialize, Serialize};
use serde_json::json;
use trady_application::sentiment::{SentimentProvider, text_sha256, validate_probabilities};
use trady_domain::{Error, Result, require, sentiment::*};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FinBertConfig {
    pub config_version: u32,
    pub enabled: bool,
    pub endpoint: String,
    pub model_revision: String,
    pub timeout_seconds: u64,
}

impl FinBertConfig {
    pub fn validate(&self) -> Result<()> {
        let url =
            Url::parse(&self.endpoint).map_err(|_| Error("invalid FinBERT endpoint".into()))?;
        require(
            self.config_version == 1
                && (1..=60).contains(&self.timeout_seconds)
                && self.model_revision.len() == 40
                && self.model_revision.bytes().all(|b| b.is_ascii_hexdigit())
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none()
                && url.path() == "/"
                && (url.scheme() == "https"
                    || (url.scheme() == "http"
                        && matches!(
                            url.host_str(),
                            Some("localhost" | "127.0.0.1" | "[::1]" | "finbert")
                        ))),
            "invalid FinBERT configuration; plain HTTP is limited to loopback or the finbert service",
        )
    }
}

pub struct FinBertClient {
    config: FinBertConfig,
    client: Client,
}

impl FinBertClient {
    pub fn new(config: FinBertConfig) -> Result<Self> {
        config.validate()?;
        require(config.enabled, "FinBERT is disabled")?;
        let client = Client::builder()
            .redirect(Policy::none())
            .no_proxy()
            .timeout(Duration::from_secs(config.timeout_seconds))
            .connect_timeout(Duration::from_secs(config.timeout_seconds.min(5)))
            .build()
            .map_err(|_| Error("FinBERT client initialization failed".into()))?;
        Ok(Self { config, client })
    }
}

impl SentimentProvider for FinBertClient {
    fn model_revision(&self) -> &str {
        &self.config.model_revision
    }

    fn predict(&self, documents: &[SentimentDocument]) -> Result<SentimentBatch> {
        require(
            (1..=MAX_SENTIMENT_ITEMS).contains(&documents.len()),
            "invalid sentiment batch size",
        )?;
        let mut ids = BTreeSet::new();
        for document in documents {
            require(
                !document.item_id.is_empty()
                    && document.item_id.len() <= 256
                    && ids.insert(&document.item_id)
                    && !document.text.trim().is_empty()
                    && document.text.chars().count() <= MAX_SENTIMENT_CHARS
                    && document.text_sha256 == text_sha256(&document.text),
                "invalid sentiment document",
            )?;
        }
        let endpoint = Url::parse(&self.config.endpoint)
            .and_then(|url| url.join("v1/sentiment"))
            .map_err(|_| Error("invalid FinBERT endpoint".into()))?;
        let response = self
            .client
            .post(endpoint)
            .json(&json!({"schema_version":SENTIMENT_VERSION,"documents":documents}))
            .send()
            .map_err(|_| Error("FinBERT unavailable or timed out".into()))?;
        require(
            response.status() == reqwest::StatusCode::OK,
            "FinBERT returned a non-success status",
        )?;
        let mut bytes = Vec::new();
        response
            .take(65537)
            .read_to_end(&mut bytes)
            .map_err(|_| Error("FinBERT response read failed".into()))?;
        require(bytes.len() <= 65536, "FinBERT response exceeds size bound")?;
        let batch: SentimentBatch = serde_json::from_slice(&bytes)
            .map_err(|_| Error("invalid FinBERT response schema".into()))?;
        require(
            batch.schema_version == SENTIMENT_VERSION
                && batch.model_id == FINBERT_MODEL
                && batch.model_revision == self.config.model_revision
                && batch.predictions.len() == documents.len(),
            "FinBERT response identity or batch mismatch",
        )?;
        let mut seen = BTreeSet::new();
        for prediction in &batch.predictions {
            require(
                seen.insert(&prediction.item_id)
                    && documents.iter().any(|d| {
                        d.item_id == prediction.item_id && d.text_sha256 == prediction.text_sha256
                    }),
                "FinBERT response item or hash mismatch",
            )?;
            validate_probabilities(&prediction.probabilities)?;
        }
        Ok(batch)
    }
}
