//! Deterministic per-context UCB1 over direct and parallel analysis.
//! Counts are external-verifier outcomes, not model-provided scores or weight training.
//! No epsilon probability is implied: ties and untried arms use direct-first order.
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

const MAX_CONTEXTS: usize = 1024;
const MAX_CONTEXT_BYTES: usize = 256;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Strategy {
    Direct,
    Parallel,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Arm {
    trials: u64,
    successes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    #[serde(deserialize_with = "unique_contexts")]
    contexts: std::collections::BTreeMap<String, [Arm; 2]>,
}

fn unique_contexts<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<std::collections::BTreeMap<String, [Arm; 2]>, D::Error> {
    struct ContextVisitor;
    impl<'de> serde::de::Visitor<'de> for ContextVisitor {
        type Value = std::collections::BTreeMap<String, [Arm; 2]>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a map with unique context keys")
        }
        fn visit_map<M: serde::de::MapAccess<'de>>(
            self,
            mut map: M,
        ) -> Result<Self::Value, M::Error> {
            let mut contexts = Self::Value::new();
            while let Some((key, arms)) = map.next_entry()? {
                if contexts.insert(key, arms).is_some() {
                    return Err(serde::de::Error::custom("duplicate policy context"));
                }
            }
            Ok(contexts)
        }
    }
    deserializer.deserialize_map(ContextVisitor)
}

impl Policy {
    pub fn load(path: &Path) -> Result<Self, String> {
        let file = match File::open(path) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(format!("open policy: {e}")),
        };
        const MAX_BYTES: u64 = 2 * 1024 * 1024;
        let mut bytes = Vec::new();
        file.take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("read policy: {e}"))?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err("policy file exceeds 2 MiB".into());
        }
        let policy: Self =
            serde_json::from_slice(&bytes).map_err(|e| format!("decode policy: {e}"))?;
        policy.validate()?;
        Ok(policy)
    }

    /// Caller must hold the orchestrator's exclusive state lock across load/update/save.
    /// A parent-directory sync failure can occur after the new state is visible.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let bytes = serde_json::to_vec(self).map_err(|e| format!("encode policy: {e}"))?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let temp = parent.join(format!(
            ".policy-{}-{}.tmp",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temp)
            .map_err(|e| format!("create policy temporary file: {e}"))?;
        let result = (|| -> std::io::Result<()> {
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::rename(&temp, path)?;
            File::open(parent)?.sync_all()
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result.map_err(|e| format!("persist policy: {e}"))
    }

    fn validate_context(context: &str) -> Result<(), String> {
        if context.trim().is_empty() || context.len() > MAX_CONTEXT_BYTES {
            return Err("context must contain non-whitespace and be at most 256 bytes".into());
        }
        Ok(())
    }

    fn validate_capacity(&self, context: &str) -> Result<(), String> {
        if self.contexts.len() >= MAX_CONTEXTS && !self.contexts.contains_key(context) {
            return Err("policy context capacity exhausted".into());
        }
        Ok(())
    }

    fn validate(&self) -> Result<(), String> {
        if self.contexts.len() > MAX_CONTEXTS {
            return Err("too many policy contexts".into());
        }
        for (context, arms) in &self.contexts {
            Self::validate_context(context)?;
            if arms.iter().any(|a| a.successes > a.trials)
                || arms[0].trials.checked_add(arms[1].trials).is_none()
            {
                return Err("invalid policy counts".into());
            }
        }
        Ok(())
    }

    /// Select without changing evidence. Repeated calls without an update are identical.
    /// Uses mean + sqrt(2 * ln(total trials) / arm trials), after exploring untried arms.
    pub fn choose(&self, context: &str) -> Result<Strategy, String> {
        self.validate()?;
        Self::validate_context(context)?;
        self.validate_capacity(context)?;
        let Some(arms) = self.contexts.get(context) else {
            return Ok(Strategy::Direct);
        };
        if arms[0].trials == 0 {
            return Ok(Strategy::Direct);
        }
        if arms[1].trials == 0 {
            return Ok(Strategy::Parallel);
        }
        let total = (arms[0].trials + arms[1].trials) as f64;
        let score = |arm: &Arm| {
            arm.successes as f64 / arm.trials as f64 + (2.0 * total.ln() / arm.trials as f64).sqrt()
        };
        Ok(if score(&arms[1]) > score(&arms[0]) {
            Strategy::Parallel
        } else {
            Strategy::Direct
        })
    }

    /// Record exactly one externally verified outcome for the executed strategy.
    /// Only the trusted orchestrator may call this; skip infrastructure failures.
    /// A boolean API does not itself authenticate the reward's provenance.
    pub fn update(
        &mut self,
        context: &str,
        strategy: Strategy,
        reward: bool,
    ) -> Result<(), String> {
        self.validate()?;
        Self::validate_context(context)?;
        self.validate_capacity(context)?;
        let arms = self.contexts.entry(context.to_owned()).or_default();
        (arms[0].trials + arms[1].trials)
            .checked_add(1)
            .ok_or("policy trial count exhausted")?;
        let arm = &mut arms[match strategy {
            Strategy::Direct => 0,
            Strategy::Parallel => 1,
        }];
        arm.trials += 1;
        arm.successes += u64::from(reward);
        Ok(())
    }
}
