//! Source-bound records, not an adjudicator of model correctness.
//! The caller supplies trusted inventory/tool-event IDs; this module does not
//! rehash artifacts or establish whether a claim follows from its cited source.

// Fixed limits apply to UTF-8 bytes, including caller-supplied limitations.
const MAX_FINDINGS: usize = 256;
const MAX_CLAIM_BYTES: usize = 8192;
const MAX_SOURCE_BYTES: usize = 4096;
const MAX_LIMITATIONS: usize = 32;
const MAX_LIMITATION_BYTES: usize = 2048;

enum SourceIndex<'a> {
    Set(std::collections::HashSet<&'a str>),
    Slice(&'a [String]),
}

impl<'a> SourceIndex<'a> {
    fn new(known_sources: &'a [String], findings_len: usize) -> Self {
        if known_sources.len() > 16 && findings_len > 1 {
            Self::Set(known_sources.iter().map(String::as_str).collect())
        } else {
            Self::Slice(known_sources)
        }
    }

    fn contains(&self, source: &str) -> bool {
        match self {
            Self::Set(set) => set.contains(source),
            Self::Slice(slice) => slice.iter().any(|s| s == source),
        }
    }
}

pub fn validate_findings(findings: &[Finding], known_sources: &[String]) -> Result<(), String> {
    let index = SourceIndex::new(known_sources, findings.len());
    validate_findings_lookup(findings, |source| index.contains(source))
}

pub(crate) fn validate_findings_lookup(
    findings: &[Finding],
    has_source: impl Fn(&str) -> bool,
) -> Result<(), String> {
    if findings.len() > MAX_FINDINGS {
        return Err("finding count limit exceeded".into());
    }
    for (index, finding) in findings.iter().enumerate() {
        if finding.claim.len() > MAX_CLAIM_BYTES
            || finding.source.len() > MAX_SOURCE_BYTES
            || finding.limitations.len() > MAX_LIMITATIONS
            || finding
                .limitations
                .iter()
                .any(|s| s.len() > MAX_LIMITATION_BYTES)
        {
            return Err(format!("finding {index}: record size limit exceeded"));
        }
        if finding.claim.trim().is_empty() {
            return Err(format!("finding {index}: empty claim"));
        }
        let unsourced_unknown =
            matches!(finding.kind, EvidenceKind::Unknown) && finding.source.is_empty();
        if !unsourced_unknown && (finding.source.trim().is_empty() || !has_source(&finding.source))
        {
            return Err(format!(
                "finding {index}: source must exactly match a known source ID"
            ));
        }
    }
    Ok(())
}
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

/// Create a new report. The caller owns the output directory; create_new
/// atomically refuses existing files, including dangling destination symlinks.
pub fn write_report(
    path: &Path,
    target: &str,
    findings: &[Finding],
    known_sources: &[String],
) -> Result<(), String> {
    let index = SourceIndex::new(known_sources, findings.len());
    write_report_checked(path, target, findings, |source| index.contains(source))
}

pub(crate) fn write_report_checked(
    path: &Path,
    target: &str,
    findings: &[Finding],
    has_source: impl Fn(&str) -> bool,
) -> Result<(), String> {
    validate_findings_lookup(findings, has_source)?;
    #[derive(Serialize)]
    struct Report<'a> {
        target: &'a str,
        coverage: &'static str,
        findings: &'a [Finding],
    }
    let report = Report {
        target,
        coverage: "Findings cover cited evidence only; not original-source recovery or universal equivalence.",
        findings,
    };
    // Serialize before creating any filesystem entry.
    let bytes = serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("create report: {e}"))?;
    file.write_all(&bytes)
        .map_err(|e| format!("write report: {e}"))?;
    file.sync_all().map_err(|e| format!("sync report: {e}"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    Observation,
    Inference,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub kind: EvidenceKind,
    pub claim: String,
    pub source: String,
    pub limitations: Vec<String>,
}
