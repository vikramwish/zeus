//! Experimental shadow-only outcome scoring. Never authorizes execution.
//! Coefficients are supplied offline; scores are not calibrated confidence.
use crate::{config::Job, policy::Strategy, workspace};
use serde::{Deserialize, Serialize};
use std::{fs::OpenOptions, io::Read, path::PathBuf};

const MAX_MODEL_BYTES: u64 = 64 * 1024;
pub const FEATURE_SCHEMA: &str = "job_limits_v1";
pub const FEATURES: [&str; 8] = [
    "task_bytes",
    "max_turns",
    "max_repairs",
    "max_model_calls",
    "has_model_call_limit",
    "log_token_limit",
    "has_token_limit",
    "job_seconds",
];

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub model_path: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
pub struct State {
    pub feature_schema: &'static str,
    pub feature_names: [&'static str; 8],
    pub features: [f64; 8],
    /// Binds estimates to this context, provider/model and fixed verifier.
    pub domain_sha256: String,
    pub verifier_configured: bool,
}
impl State {
    pub fn from_job(job: &Job) -> Self {
        let domain = serde_json::json!({"context":job.context,"model":job.model,
            "protocol":job.protocol,"endpoint":job.endpoint,"verifier":job.verifier});
        Self {
            feature_schema: FEATURE_SCHEMA,
            feature_names: FEATURES,
            features: [
                job.task.len() as f64 / 32768.0,
                job.max_turns as f64 / 32.0,
                job.autonomy.max_repairs as f64 / 8.0,
                job.autonomy.max_model_calls.unwrap_or(0) as f64 / 1024.0,
                f64::from(job.autonomy.max_model_calls.is_some()),
                (job.autonomy.max_total_tokens.unwrap_or(0) as f64).ln_1p()
                    / (u64::MAX as f64).ln_1p(),
                f64::from(job.autonomy.max_total_tokens.is_some()),
                job.max_job_seconds as f64 / 86400.0,
            ],
            domain_sha256: workspace::digest(domain.to_string().as_bytes()),
            verifier_configured: !job.verifier.is_empty(),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Head {
    training_samples: u64,
    /// Intercept followed by eight coefficients in FEATURES order.
    success_logit: [f64; 9],
    log1p_tokens: [f64; 9],
    log1p_seconds: [f64; 9],
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Artifact {
    schema_version: u32,
    feature_schema: String,
    model_version: String,
    domain_sha256: String,
    feature_min: [f64; 8],
    feature_max: [f64; 8],
    direct: Head,
    parallel: Head,
}

#[derive(Debug, Serialize)]
pub struct Estimate {
    pub strategy: Strategy,
    pub success_probability: f64,
    pub predicted_tokens: f64,
    pub predicted_seconds: f64,
    pub training_samples: u64,
}
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Decision {
    /// An experimental ranking, never an executable routing instruction.
    Rank {
        preferred: Strategy,
    },
    Abstain {
        reason: Reason,
    },
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    NoModel,
    InvalidModel,
    DomainMismatch,
    NoVerifier,
    OutsideSupport,
    InsufficientSamples,
    InvalidPrediction,
    Ambiguous,
}
#[derive(Debug, Serialize)]
pub struct Record {
    pub schema_version: u32,
    pub mode: &'static str,
    pub probability_status: &'static str,
    pub state: State,
    pub executed_strategy: Strategy,
    pub routing_source: &'static str,
    pub model_sha256: Option<String>,
    pub model_version: Option<String>,
    pub predictions: Vec<Estimate>,
    pub decision: Decision,
}

fn load(config: &Config) -> Result<(Artifact, Vec<u8>), Reason> {
    let path = config.model_path.as_ref().ok_or(Reason::NoModel)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path).map_err(|_| Reason::InvalidModel)?;
    let metadata = file.metadata().map_err(|_| Reason::InvalidModel)?;
    if !metadata.is_file() || metadata.len() > MAX_MODEL_BYTES {
        return Err(Reason::InvalidModel);
    }
    let mut bytes = Vec::new();
    file.take(MAX_MODEL_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Reason::InvalidModel)?;
    if bytes.len() as u64 > MAX_MODEL_BYTES {
        return Err(Reason::InvalidModel);
    }
    let model: Artifact = serde_json::from_slice(&bytes).map_err(|_| Reason::InvalidModel)?;
    if model.schema_version != 1
        || model.feature_schema != FEATURE_SCHEMA
        || model.model_version.is_empty()
        || model.model_version.len() > 128
        || model.domain_sha256.len() != 64
        || !model.domain_sha256.bytes().all(|b| b.is_ascii_hexdigit())
        || model
            .feature_min
            .iter()
            .zip(&model.feature_max)
            .any(|(&lo, &hi)| !lo.is_finite() || !hi.is_finite() || lo < 0.0 || hi > 1.0 || lo > hi)
        || [&model.direct, &model.parallel].iter().any(|head| {
            [&head.success_logit, &head.log1p_tokens, &head.log1p_seconds]
                .iter()
                .any(|weights| weights.iter().any(|w| !w.is_finite() || w.abs() > 100.0))
        })
    {
        return Err(Reason::InvalidModel);
    }
    Ok((model, bytes))
}

fn predict(head: &Head, features: &[f64; 8], strategy: Strategy) -> Result<Estimate, Reason> {
    let dot = |weights: &[f64; 9]| {
        weights[0]
            + weights[1..]
                .iter()
                .zip(features)
                .map(|(w, x)| w * x)
                .sum::<f64>()
    };
    let logit = dot(&head.success_logit);
    let tokens = dot(&head.log1p_tokens);
    let seconds = dot(&head.log1p_seconds);
    if !logit.is_finite() || !(0.0..=30.0).contains(&tokens) || !(0.0..=30.0).contains(&seconds) {
        return Err(Reason::InvalidPrediction);
    }
    Ok(Estimate {
        strategy,
        success_probability: 1.0 / (1.0 + (-logit).exp()),
        predicted_tokens: tokens.exp_m1(),
        predicted_seconds: seconds.exp_m1(),
        training_samples: head.training_samples,
    })
}

/// Pure scoring after a bounded artifact read. Returned bytes are the exact
/// accepted artifact, retained by the caller for reproducibility before execution.
pub fn evaluate(job: &Job, config: &Config, strategy: Strategy) -> (Record, Option<Vec<u8>>) {
    let mut record = Record {
        schema_version: 1,
        mode: "shadow",
        probability_status: "uncalibrated",
        state: State::from_job(job),
        executed_strategy: strategy,
        routing_source: if job.strategy.is_some() {
            "explicit"
        } else {
            "ucb"
        },
        model_sha256: None,
        model_version: None,
        predictions: vec![],
        decision: Decision::Abstain {
            reason: Reason::NoModel,
        },
    };
    let (model, bytes) = match load(config) {
        Ok(loaded) => loaded,
        Err(reason) => {
            record.decision = Decision::Abstain { reason };
            return (record, None);
        }
    };
    record.model_sha256 = Some(workspace::digest(&bytes));
    record.model_version = Some(model.model_version.clone());
    let score =
        || -> Result<(Vec<Estimate>, Decision), Reason> {
            if model.domain_sha256 != record.state.domain_sha256 {
                return Err(Reason::DomainMismatch);
            }
            if !record.state.verifier_configured {
                return Err(Reason::NoVerifier);
            }
            if record.state.features.iter().enumerate().any(|(i, x)| {
                !x.is_finite() || *x < model.feature_min[i] || *x > model.feature_max[i]
            }) {
                return Err(Reason::OutsideSupport);
            }
            let direct = predict(&model.direct, &record.state.features, Strategy::Direct)?;
            let parallel = predict(&model.parallel, &record.state.features, Strategy::Parallel)?;
            let decision = if direct.training_samples < 30 || parallel.training_samples < 30 {
                Decision::Abstain {
                    reason: Reason::InsufficientSamples,
                }
            } else if (direct.success_probability - parallel.success_probability).abs() < 0.05 {
                Decision::Abstain {
                    reason: Reason::Ambiguous,
                }
            } else {
                Decision::Rank {
                    preferred: if direct.success_probability > parallel.success_probability {
                        Strategy::Direct
                    } else {
                        Strategy::Parallel
                    },
                }
            };
            Ok((vec![direct, parallel], decision))
        };
    match score() {
        Ok((predictions, decision)) => {
            record.predictions = predictions;
            record.decision = decision;
        }
        Err(reason) => record.decision = Decision::Abstain { reason },
    }
    (record, Some(bytes))
}
