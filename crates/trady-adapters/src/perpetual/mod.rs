//! Pinned native Perpetual inference from deployment-owned immutable artifacts.

use chrono::{DateTime, Duration, Utc};
use perpetual::{Matrix, PerpetualBooster, booster::config::BoosterIO, objective::Objective};
use rust_decimal::Decimal;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use trady_application::perpetual::{AdvisoryOutput, PerpetualAdvisor};
use trady_domain::{Error, Result, perpetual::*, require};

pub const RUNTIME_VERSION: &str = "3.0.0-rc.2";
pub const RUNTIME_SOURCE_REVISION: &str = "6cddc9e55a7b132f7f252fca7617d95e4c4538f8";
pub const MODEL_FORMAT: &str = "perpetual-json-3.0.0-rc.2-binary-platt-v1";
pub const MAX_ARTIFACT_BYTES: usize = 16 * 1024 * 1024;

/// Only the composition root supplies artifacts, from its reviewed deployment
/// configuration. This type is deliberately absent from LLM tool arguments.
pub struct ApprovedArtifact {
    pub manifest: ModelManifest,
    pub bytes: Vec<u8>,
}

struct LoadedModel {
    manifest: ModelManifest,
    booster: PerpetualBooster,
}

/// No mutation, fitting, download, model-path selection or promotion operations.
/// One-row inference runs serially and is bounded by artifact/tree/node limits.
pub struct NativePerpetualAdvisor {
    models: BTreeMap<String, LoadedModel>,
}

impl NativePerpetualAdvisor {
    /// Fail startup on any invalid, unapproved, incompatible or corrupt artifact.
    /// An empty registry is allowed, and always returns persisted UNAVAILABLE advice.
    pub fn from_approved_artifacts(
        artifacts: Vec<ApprovedArtifact>,
        now: DateTime<Utc>,
    ) -> Result<Self> {
        require(artifacts.len() <= 32, "too many approved perpetual models")?;
        let mut models = BTreeMap::new();
        for artifact in artifacts {
            let m = artifact.manifest;
            m.validate(now)?;
            require(
                m.runtime_version == RUNTIME_VERSION
                    && m.runtime_source_revision == RUNTIME_SOURCE_REVISION
                    && m.model_format == MODEL_FORMAT,
                "unsupported perpetual runtime, source revision or model format",
            )?;
            require(
                !artifact.bytes.is_empty() && artifact.bytes.len() <= MAX_ARTIFACT_BYTES,
                "perpetual artifact exceeds inference resource bounds",
            )?;
            let digest = artifact_sha256(&artifact.bytes);
            require(
                digest == m.artifact_sha256,
                "perpetual artifact SHA-256 mismatch",
            )?;
            let json = std::str::from_utf8(&artifact.bytes)
                .map_err(|_| Error("perpetual artifact is not UTF-8 JSON".into()))?;
            let booster = PerpetualBooster::from_json(json)
                .map_err(|e| Error(format!("cannot load pinned perpetual artifact: {e}")))?;
            validate_booster(&booster, &m)?;
            require(
                !models.contains_key(&m.model_revision_id),
                "duplicate immutable perpetual model revision",
            )?;
            models.insert(
                m.model_revision_id.clone(),
                LoadedModel {
                    manifest: m,
                    booster,
                },
            );
        }
        Ok(Self { models })
    }

    pub fn model_count(&self) -> usize {
        self.models.len()
    }
}

fn validate_booster(booster: &PerpetualBooster, manifest: &ModelManifest) -> Result<()> {
    require(
        matches!(booster.cfg.objective, Objective::LogLoss)
            && booster.base_score.is_finite()
            && booster.isotonic_calibrator.is_none()
            && booster.regression_head.is_none()
            && booster.cal_models.is_empty()
            && booster.cal_params.is_empty()
            && !booster.trees.is_empty()
            && booster.trees.len() <= 1024,
        "unsupported perpetual objective, calibration payload or tree count",
    )?;
    let names = manifest
        .feature_schema
        .features
        .iter()
        .map(|f| f.name.as_str())
        .collect::<Vec<_>>();
    let names = serde_json::to_string(&names).map_err(|e| Error(e.to_string()))?;
    require(
        booster.metadata.get("trady.feature_schema_version")
            == Some(&manifest.feature_schema.version)
            && booster.metadata.get("trady.feature_names") == Some(&names)
            && booster.metadata.get("trady.label_version") == Some(&manifest.label_version),
        "native model metadata does not match ordered feature or label schema",
    )?;
    let mut node_count = 0usize;
    for tree in &booster.trees {
        node_count = node_count
            .checked_add(tree.nodes.len())
            .ok_or_else(|| Error("perpetual node count overflow".into()))?;
        require(
            node_count <= 65_536 && tree.depth <= 64 && tree.nodes.contains_key(&0),
            "perpetual tree exceeds inference bounds or has no root",
        )?;
        // Native traversal assumes a valid graph. Validate it before inference to
        // exclude absent children, cycles and feature-index panics.
        let mut seen = BTreeSet::new();
        let mut pending = vec![(0usize, 0usize)];
        while let Some((id, depth)) = pending.pop() {
            require(
                depth <= 64 && seen.insert(id),
                "cyclic or shared-child perpetual tree",
            )?;
            let node = tree
                .nodes
                .get(&id)
                .ok_or_else(|| Error("perpetual tree child does not exist".into()))?;
            require(
                node.num == id
                    && node.weight_value.is_finite()
                    && node
                        .leaf_weights
                        .is_none_or(|w| w.iter().all(|x| x.is_finite()))
                    && node
                        .stats
                        .as_ref()
                        .is_none_or(|s| s.weights.iter().all(|w| w.is_finite())),
                "invalid perpetual node identity or non-finite leaf weights",
            )?;
            if !node.is_leaf {
                require(
                    node.split_feature < manifest.feature_schema.features.len()
                        && node.split_value.is_finite()
                        && node.left_cats.is_none(),
                    "unsupported perpetual split feature, threshold or category",
                )?;
                let children = [node.left_child, node.right_child, node.missing_node]
                    .into_iter()
                    .collect::<BTreeSet<_>>();
                require(
                    children.len() >= 2,
                    "perpetual split has fewer than two children",
                )?;
                pending.extend(children.into_iter().map(|child| (child, depth + 1)));
            }
        }
        require(
            seen.len() == tree.nodes.len(),
            "perpetual artifact contains unreachable nodes",
        )?;
    }
    Ok(())
}

impl PerpetualAdvisor for NativePerpetualAdvisor {
    fn assess(
        &self,
        id: &str,
        context: &AssessmentContext,
        now: DateTime<Utc>,
    ) -> Result<AdvisoryOutput> {
        require(valid_reference(id), "invalid perpetual assessment ID")?;
        let unavailable = |reason: &str| AdvisoryOutput {
            assessment: Assessment::unavailable(
                id.into(),
                &context.scenario.binding.arguments,
                now,
                reason,
            ),
            model: None,
        };
        if let Err(e) = context.validate(now) {
            return Ok(unavailable(&e.0));
        }
        let supported: Vec<_> = self
            .models
            .values()
            .filter(|m| m.manifest.supports(context))
            .collect();
        if supported.is_empty() {
            return Ok(unavailable(
                "No approved model supports the persisted scenario and feature schema",
            ));
        }
        if supported.len() != 1 {
            return Ok(unavailable(
                "Ambiguous deployment registry; a unique supported model is required",
            ));
        }
        let loaded = supported[0];
        let m = &loaded.manifest;
        if let Err(e) = m.validate(now) {
            return Ok(unavailable(&e.0));
        }
        let drift = match m.check_features(context, now) {
            Ok(value) => value,
            Err(e) => return Ok(unavailable(&e.0)),
        };
        let mut assessment = Assessment::unavailable(
            id.into(),
            &context.scenario.binding.arguments,
            now,
            "Prediction not yet available",
        );
        assessment.feature_snapshot_id = Some(context.features.snapshot_id.clone());
        assessment.model_revision_id = Some(m.model_revision_id.clone());
        assessment.training_data_cutoff_at = Some(m.training.training_data_cutoff_at);
        assessment.validation_report_id = Some(m.validation_report_id.clone());
        assessment.calibration_status = CalibrationStatus::Validated;
        assessment.drift_status = drift;
        if drift != DriftStatus::InRange {
            assessment.status = AssessmentStatus::Abstain;
            assessment.limitations = vec!["Label-free feature drift or support bounds require reassessment; no actionable prediction".into()];
            return Ok(AdvisoryOutput {
                assessment,
                model: Some(m.clone()),
            });
        }
        let values: Vec<_> = context.features.features.iter().map(|f| f.value).collect();
        // Matrix is column-major; with exactly one row the ordered vector is
        // also the column-major layout. Use the native classification path,
        // which averages fold probabilities in this pinned version.
        let matrix = Matrix::new(&values, 1, values.len());
        let probabilities = loaded.booster.predict_proba(&matrix, false, false);
        require(
            probabilities.len() == 1,
            "native perpetual returned the wrong prediction shape",
        )?;
        let probability = calibrated_probability(probabilities[0], &m.calibration)?;
        let probability = format!("{probability:.16}")
            .parse::<Decimal>()
            .map_err(|_| Error("cannot represent perpetual probability".into()))?;
        let ttl = now
            .checked_add_signed(Duration::seconds(i64::from(m.max_assessment_age_seconds)))
            .ok_or_else(|| Error("perpetual assessment expiry overflow".into()))?;
        let feature_expiry = m
            .feature_schema
            .features
            .iter()
            .zip(&context.features.features)
            .flat_map(|(spec, value)| {
                value.lineage.iter().map(move |s| {
                    s.observed_at
                        .checked_add_signed(Duration::seconds(i64::from(spec.max_age_seconds)))
                })
            })
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| Error("perpetual feature expiry overflow".into()))?;
        assessment.valid_until = [
            ttl,
            m.valid_until,
            context.features.valid_until,
            context.scenario.valid_until,
            context.scenario.market.valid_until,
            context.scenario.binding.deadline_at,
        ]
        .into_iter()
        .chain(feature_expiry)
        .min()
        .ok_or_else(|| Error("missing perpetual expiry".into()))?;
        if assessment.valid_until <= now {
            return Ok(unavailable("Perpetual input validity is exhausted"));
        }
        assessment.status = AssessmentStatus::Ok;
        assessment.target_hit_probability = Some(probability);
        assessment.limitations = vec!["Advisory target-hit classification only; no validated return interval or causal explanation".into(),
            "Label-free drift checks do not establish unchanged realized predictive quality".into()];
        assessment.validate()?;
        Ok(AdvisoryOutput {
            assessment,
            model: Some(m.clone()),
        })
    }
}

fn calibrated_probability(probability: f64, calibration: &ProbabilityCalibration) -> Result<f64> {
    require(
        probability.is_finite() && (0.0..=1.0).contains(&probability),
        "native perpetual returned a non-probability",
    )?;
    // Clamp only for the finite logit transform; sigmoid is stable for both signs.
    let p = probability.clamp(f64::EPSILON, 1.0 - f64::EPSILON);
    let z = calibration.slope * (p.ln() - (-p).ln_1p()) + calibration.intercept;
    require(z.is_finite(), "perpetual calibration overflow")?;
    let result = if z >= 0.0 {
        1.0 / (1.0 + (-z).exp())
    } else {
        let e = z.exp();
        e / (1.0 + e)
    };
    require(
        result.is_finite() && (0.0..=1.0).contains(&result),
        "invalid calibrated perpetual probability",
    )?;
    Ok(result)
}

fn artifact_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests;
