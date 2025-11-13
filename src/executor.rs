use crate::bot::TestBot;
use crate::test_spec::{ActionType, TestSpec, TimelineEntry};
use anyhow::Result;
use colored::Colorize;
use std::collections::HashMap;

pub struct TestExecutor {
    bot: TestBot,
}

impl TestExecutor {
    pub fn new() -> Self {
        Self {
            bot: TestBot::new(),
        }
    }

    pub async fn connect(&mut self, server: &str) -> Result<()> {
        self.bot.connect(server).await
    }

    pub async fn run_test(&mut self, test: &TestSpec) -> Result<TestResult> {
        println!("\n{} {}", "Running test:".cyan().bold(), test.name.bold());
        if let Some(desc) = &test.description {
            println!("  {}", desc.dimmed());
        }

        let max_tick = test.max_tick();
        println!("  Timeline: {} ticks\n", max_tick);

        // Clean up test area if specified
        if let Some(region) = test.cleanup_region() {
            println!("  {} Cleaning test area...", "→".blue());
            let cmd = format!(
                "fill {} {} {} {} {} {} air",
                region[0][0], region[0][1], region[0][2],
                region[1][0], region[1][1], region[1][2]
            );
            self.bot.send_command(&cmd).await?;
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        }

        // Expand timeline entries with multiple ticks into separate entries
        let mut expanded_entries = Vec::new();
        for entry in &test.timeline {
            let ticks = entry.at.to_vec();
            for (idx, tick) in ticks.iter().enumerate() {
                expanded_entries.push((*tick, entry, idx));
            }
        }

        // Group by tick
        let mut actions_by_tick: HashMap<u32, Vec<(&TimelineEntry, usize)>> = HashMap::new();
        for (tick, entry, idx) in expanded_entries {
            actions_by_tick
                .entry(tick)
                .or_insert_with(Vec::new)
                .push((entry, idx));
        }

        // Freeze time
        self.bot.send_command("tick freeze").await?;
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let mut current_tick = 0;
        let mut passed = 0;
        let mut failed = 0;

        // Execute actions tick by tick
        while current_tick <= max_tick {
            if let Some(entries) = actions_by_tick.get(&current_tick) {
                for (entry, value_idx) in entries {
                    match self.execute_action(current_tick, entry, *value_idx).await {
                        Ok(true) => {
                            passed += 1;
                        }
                        Ok(false) => {
                            // Non-assertion action
                        }
                        Err(e) => {
                            failed += 1;
                            println!(
                                "    {} Tick {}: {}",
                                "✗".red().bold(),
                                current_tick,
                                e.to_string().red()
                            );
                        }
                    }
                }
            }

            // Step to next tick
            if current_tick < max_tick {
                self.bot.send_command("tick step 1").await?;
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            }
            current_tick += 1;
        }

        // Unfreeze time
        self.bot.send_command("tick unfreeze").await?;

        // Clean up test area after test
        if let Some(region) = test.cleanup_region() {
            println!("\n  {} Cleaning up test area...", "→".blue());
            let cmd = format!(
                "fill {} {} {} {} {} {} air",
                region[0][0], region[0][1], region[0][2],
                region[1][0], region[1][1], region[1][2]
            );
            self.bot.send_command(&cmd).await?;
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        }

        let success = failed == 0;
        println!();
        if success {
            println!("  {} Test passed: {} assertions", "✓".green().bold(), passed);
        } else {
            println!(
                "  {} Test failed: {} passed, {} failed",
                "✗".red().bold(),
                passed,
                failed
            );
        }

        Ok(TestResult {
            test_name: test.name.clone(),
            passed,
            failed,
            success,
        })
    }

    async fn execute_action(&mut self, tick: u32, entry: &TimelineEntry, value_idx: usize) -> Result<bool> {
        match &entry.action_type {
            ActionType::Place { pos, block } => {
                let cmd = format!("setblock {} {} {} {}", pos[0], pos[1], pos[2], block);
                self.bot.send_command(&cmd).await?;
                println!(
                    "    {} Tick {}: place at [{}, {}, {}] = {}",
                    "→".blue(),
                    tick,
                    pos[0],
                    pos[1],
                    pos[2],
                    block.dimmed()
                );
                Ok(false)
            }

            ActionType::PlaceEach { blocks } => {
                for placement in blocks {
                    let cmd = format!(
                        "setblock {} {} {} {}",
                        placement.pos[0], placement.pos[1], placement.pos[2], placement.block
                    );
                    self.bot.send_command(&cmd).await?;
                    println!(
                        "    {} Tick {}: place at [{}, {}, {}] = {}",
                        "→".blue(),
                        tick,
                        placement.pos[0],
                        placement.pos[1],
                        placement.pos[2],
                        placement.block.dimmed()
                    );
                    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                }
                Ok(false)
            }

            ActionType::Fill { region, with } => {
                let cmd = format!(
                    "fill {} {} {} {} {} {} {}",
                    region[0][0], region[0][1], region[0][2],
                    region[1][0], region[1][1], region[1][2],
                    with
                );
                self.bot.send_command(&cmd).await?;
                println!(
                    "    {} Tick {}: fill [{},{},{}] to [{},{},{}] = {}",
                    "→".blue(),
                    tick,
                    region[0][0],
                    region[0][1],
                    region[0][2],
                    region[1][0],
                    region[1][1],
                    region[1][2],
                    with.dimmed()
                );
                Ok(false)
            }

            ActionType::Remove { pos } => {
                let cmd = format!("setblock {} {} {} air", pos[0], pos[1], pos[2]);
                self.bot.send_command(&cmd).await?;
                println!(
                    "    {} Tick {}: remove at [{}, {}, {}]",
                    "→".blue(),
                    tick,
                    pos[0],
                    pos[1],
                    pos[2]
                );
                Ok(false)
            }

            ActionType::Assert { checks } => {
                // Wait a moment for server to send block update
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

                for check in checks {
                    let actual_block = self.bot.get_block(check.pos).await?;

                    let expected_name = check.is.trim_start_matches("minecraft:");
                    let success = if let Some(ref actual) = actual_block {
                        let actual_lower = actual.to_lowercase();
                        let expected_lower = expected_name.to_lowercase().replace("_", "");
                        actual_lower.contains(&expected_lower) ||
                        actual_lower.replace("_", "").contains(&expected_lower)
                    } else {
                        false
                    };

                    if success {
                        println!(
                            "    {} Tick {}: assert block at [{}, {}, {}] is {}",
                            "✓".green(),
                            tick,
                            check.pos[0],
                            check.pos[1],
                            check.pos[2],
                            check.is.dimmed()
                        );
                    } else {
                        anyhow::bail!(
                            "Block at [{}, {}, {}] is not {} (got {:?})",
                            check.pos[0],
                            check.pos[1],
                            check.pos[2],
                            check.is,
                            actual_block
                        );
                    }
                }
                Ok(true)
            }

            ActionType::AssertState { pos, state, values } => {
                // Wait a moment for server to send block update
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

                let actual_value = self.bot.get_block_state_property(*pos, state).await?;
                let expected_value = &values[value_idx];

                let success = if let Some(ref actual) = actual_value {
                    actual.contains(expected_value)
                } else {
                    false
                };

                if success {
                    println!(
                        "    {} Tick {}: assert block at [{}, {}, {}] state {} = {}",
                        "✓".green(),
                        tick,
                        pos[0],
                        pos[1],
                        pos[2],
                        state.dimmed(),
                        expected_value.dimmed()
                    );
                    Ok(true)
                } else {
                    anyhow::bail!(
                        "Block at [{}, {}, {}] state {} is not {} (got {:?})",
                        pos[0],
                        pos[1],
                        pos[2],
                        state,
                        expected_value,
                        actual_value
                    );
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct TestResult {
    pub test_name: String,
    pub passed: usize,
    pub failed: usize,
    pub success: bool,
}
