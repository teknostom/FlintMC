use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestSpec {
    #[serde(default)]
    pub flint_version: Option<String>,
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub setup: Option<SetupSpec>,
    pub timeline: Vec<TimelineEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupSpec {
    pub cleanup: Option<CleanupSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupSpec {
    pub region: [[i32; 3]; 2],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEntry {
    #[serde(rename = "at")]
    pub at: TickSpec,
    #[serde(rename = "do")]
    pub action_name: String,
    #[serde(flatten)]
    pub action_type: ActionType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TickSpec {
    Single(u32),
    Multiple(Vec<u32>),
}

impl TickSpec {
    pub fn to_vec(&self) -> Vec<u32> {
        match self {
            TickSpec::Single(t) => vec![*t],
            TickSpec::Multiple(v) => v.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ActionType {
    Place {
        pos: [i32; 3],
        block: String,
    },
    PlaceEach {
        blocks: Vec<BlockPlacement>,
    },
    Fill {
        region: [[i32; 3]; 2],
        with: String,
    },
    Remove {
        pos: [i32; 3],
    },
    Assert {
        checks: Vec<BlockCheck>,
    },
    AssertState {
        pos: [i32; 3],
        state: String,
        values: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockPlacement {
    pub pos: [i32; 3],
    pub block: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockCheck {
    pub pos: [i32; 3],
    pub is: String,
}

impl TestSpec {
    pub fn from_file(path: &PathBuf) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let spec: TestSpec = serde_json::from_str(&content)?;
        Ok(spec)
    }

    pub fn max_tick(&self) -> u32 {
        self.timeline
            .iter()
            .flat_map(|entry| entry.at.to_vec())
            .max()
            .unwrap_or(0)
    }

    pub fn cleanup_region(&self) -> Option<[[i32; 3]; 2]> {
        self.setup
            .as_ref()
            .and_then(|s| s.cleanup.as_ref())
            .map(|c| c.region)
    }
}
