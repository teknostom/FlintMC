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
    pub cleanup: CleanupSpec,
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
    // Maximum allowed test dimensions
    pub const MAX_WIDTH: i32 = 15;
    pub const MAX_HEIGHT: i32 = 384;
    pub const MAX_DEPTH: i32 = 15;

    pub fn from_file(path: &PathBuf) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let spec: TestSpec = serde_json::from_str(&content)?;
        spec.validate()?;
        Ok(spec)
    }

    pub fn max_tick(&self) -> u32 {
        self.timeline
            .iter()
            .flat_map(|entry| entry.at.to_vec())
            .max()
            .unwrap_or(0)
    }

    pub fn cleanup_region(&self) -> [[i32; 3]; 2] {
        self.setup
            .as_ref()
            .map(|s| s.cleanup.region)
            .expect("Cleanup region is required but not present")
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        // Ensure setup with cleanup is present
        let setup = self.setup.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Test '{}' missing required 'setup' section", self.name))?;

        let region = setup.cleanup.region;
        let min = region[0];
        let max = region[1];

        // Calculate dimensions
        let width = max[0] - min[0] + 1;
        let height = max[1] - min[1] + 1;
        let depth = max[2] - min[2] + 1;

        // Validate region forms valid bounds
        if min[0] > max[0] || min[1] > max[1] || min[2] > max[2] {
            anyhow::bail!(
                "Test '{}': Invalid cleanup region - min coordinates must be <= max coordinates. Got min=[{},{},{}], max=[{},{},{}]",
                self.name, min[0], min[1], min[2], max[0], max[1], max[2]
            );
        }

        // Validate dimensions don't exceed max size
        if width > Self::MAX_WIDTH {
            anyhow::bail!(
                "Test '{}': Cleanup region width {} exceeds maximum {}",
                self.name, width, Self::MAX_WIDTH
            );
        }
        if height > Self::MAX_HEIGHT {
            anyhow::bail!(
                "Test '{}': Cleanup region height {} exceeds maximum {}",
                self.name, height, Self::MAX_HEIGHT
            );
        }
        if depth > Self::MAX_DEPTH {
            anyhow::bail!(
                "Test '{}': Cleanup region depth {} exceeds maximum {}",
                self.name, depth, Self::MAX_DEPTH
            );
        }

        // Validate all test coordinates are within cleanup region
        for entry in &self.timeline {
            match &entry.action_type {
                ActionType::Place { pos, .. } => {
                    self.validate_position(*pos, &region)?;
                }
                ActionType::PlaceEach { blocks } => {
                    for block in blocks {
                        self.validate_position(block.pos, &region)?;
                    }
                }
                ActionType::Fill { region: fill_region, .. } => {
                    self.validate_position(fill_region[0], &region)?;
                    self.validate_position(fill_region[1], &region)?;
                }
                ActionType::Remove { pos } => {
                    self.validate_position(*pos, &region)?;
                }
                ActionType::Assert { checks } => {
                    for check in checks {
                        self.validate_position(check.pos, &region)?;
                    }
                }
                ActionType::AssertState { pos, .. } => {
                    self.validate_position(*pos, &region)?;
                }
            }
        }

        Ok(())
    }

    fn validate_position(&self, pos: [i32; 3], region: &[[i32; 3]; 2]) -> anyhow::Result<()> {
        let min = region[0];
        let max = region[1];

        if pos[0] < min[0] || pos[0] > max[0] ||
           pos[1] < min[1] || pos[1] > max[1] ||
           pos[2] < min[2] || pos[2] > max[2] {
            anyhow::bail!(
                "Test '{}': Position [{},{},{}] is outside cleanup region [{},{},{}] to [{},{},{}]",
                self.name,
                pos[0], pos[1], pos[2],
                min[0], min[1], min[2],
                max[0], max[1], max[2]
            );
        }
        Ok(())
    }
}
