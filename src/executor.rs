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

    fn apply_offset(&self, pos: [i32; 3], offset: [i32; 3]) -> [i32; 3] {
        [
            pos[0] + offset[0],
            pos[1] + offset[1],
            pos[2] + offset[2],
        ]
    }

    /// Poll for a block at the given position with retries
    /// This handles timing issues in CI environments where block updates may take longer
    async fn poll_block_with_retry(&self, world_pos: [i32; 3], expected_block: &str, max_attempts: u32, delay_ms: u64) -> Result<Option<String>> {
        let expected_name = expected_block.trim_start_matches("minecraft:").to_lowercase().replace("_", "");

        for attempt in 0..max_attempts {
            let block = self.bot.get_block(world_pos).await?;

            // Check if the block matches what we expect
            if let Some(ref actual) = block {
                let actual_lower = actual.to_lowercase();
                if actual_lower.contains(&expected_name) || actual_lower.replace("_", "").contains(&expected_name) {
                    return Ok(block);
                }
            }

            // If not the last attempt, wait before retrying
            if attempt < max_attempts - 1 {
                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
            }
        }

        // Return whatever we have after all retries
        self.bot.get_block(world_pos).await
    }

    /// Poll for a block state property at the given position with retries
    async fn poll_block_state_with_retry(&self, world_pos: [i32; 3], state: &str, max_attempts: u32, delay_ms: u64) -> Result<Option<String>> {
        for attempt in 0..max_attempts {
            let state_value = self.bot.get_block_state_property(world_pos, state).await?;
            if state_value.is_some() {
                return Ok(state_value);
            }
            if attempt < max_attempts - 1 {
                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
            }
        }
        Ok(None)
    }

    pub async fn connect(&mut self, server: &str) -> Result<()> {
        self.bot.connect(server).await
    }

    pub async fn run_tests_parallel(&mut self, tests_with_offsets: &[(TestSpec, [i32; 3])]) -> Result<Vec<TestResult>> {
        println!("{} Running {} tests in parallel\n", "→".blue().bold(), tests_with_offsets.len());

        // Build global merged timeline
        let mut global_timeline: HashMap<u32, Vec<(usize, &TimelineEntry, usize)>> = HashMap::new();
        let mut max_global_tick = 0;

        for (test_idx, (test, _offset)) in tests_with_offsets.iter().enumerate() {
            let max_tick = test.max_tick();
            if max_tick > max_global_tick {
                max_global_tick = max_tick;
            }

            // Expand timeline entries with multiple ticks
            for entry in &test.timeline {
                let ticks = entry.at.to_vec();
                for (value_idx, tick) in ticks.iter().enumerate() {
                    global_timeline
                        .entry(*tick)
                        .or_insert_with(Vec::new)
                        .push((test_idx, entry, value_idx));
                }
            }
        }

        println!("  Global timeline: {} ticks", max_global_tick);
        println!("  {} unique tick steps with actions\n", global_timeline.len());

        // Clean all test areas before starting
        println!("{} Cleaning all test areas...", "→".blue());
        for (_test_idx, (test, offset)) in tests_with_offsets.iter().enumerate() {
            let region = test.cleanup_region();
            let world_min = self.apply_offset(region[0], *offset);
            let world_max = self.apply_offset(region[1], *offset);
            let cmd = format!(
                "fill {} {} {} {} {} {} air",
                world_min[0], world_min[1], world_min[2],
                world_max[0], world_max[1], world_max[2]
            );
            self.bot.send_command(&cmd).await?;
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        // Freeze time globally
        self.bot.send_command("tick freeze").await?;
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Track results per test
        let mut test_results: Vec<(usize, usize)> = vec![(0, 0); tests_with_offsets.len()]; // (passed, failed)

        // Execute merged timeline
        let mut current_tick = 0;
        while current_tick <= max_global_tick {
            if let Some(entries) = global_timeline.get(&current_tick) {
                for (test_idx, entry, value_idx) in entries {
                    let (test, offset) = &tests_with_offsets[*test_idx];

                    match self.execute_action(current_tick, entry, *value_idx, *offset).await {
                        Ok(true) => {
                            test_results[*test_idx].0 += 1; // increment passed
                        }
                        Ok(false) => {
                            // Non-assertion action
                        }
                        Err(e) => {
                            test_results[*test_idx].1 += 1; // increment failed
                            println!(
                                "    {} [{}] Tick {}: {}",
                                "✗".red().bold(),
                                test.name,
                                current_tick,
                                e.to_string().red()
                            );
                        }
                    }
                }
            }

            // Step to next tick
            if current_tick < max_global_tick {
                self.bot.send_command("tick step 1").await?;
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            }
            current_tick += 1;
        }

        // Unfreeze time
        self.bot.send_command("tick unfreeze").await?;

        // Clean all test areas after completion
        println!("\n{} Cleaning up all test areas...", "→".blue());
        for (_test_idx, (test, offset)) in tests_with_offsets.iter().enumerate() {
            let region = test.cleanup_region();
            let world_min = self.apply_offset(region[0], *offset);
            let world_max = self.apply_offset(region[1], *offset);
            let cmd = format!(
                "fill {} {} {} {} {} {} air",
                world_min[0], world_min[1], world_min[2],
                world_max[0], world_max[1], world_max[2]
            );
            self.bot.send_command(&cmd).await?;
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        // Build results
        let results: Vec<TestResult> = tests_with_offsets
            .iter()
            .enumerate()
            .map(|(idx, (test, _))| {
                let (passed, failed) = test_results[idx];
                let success = failed == 0;

                println!();
                if success {
                    println!("  {} [{}] Test passed: {} assertions", "✓".green().bold(), test.name, passed);
                } else {
                    println!(
                        "  {} [{}] Test failed: {} passed, {} failed",
                        "✗".red().bold(),
                        test.name,
                        passed,
                        failed
                    );
                }

                TestResult {
                    test_name: test.name.clone(),
                    passed,
                    failed,
                    success,
                }
            })
            .collect();

        Ok(results)
    }

    #[allow(dead_code)]
    pub async fn run_test(&mut self, test: &TestSpec) -> Result<TestResult> {
        self.run_test_with_offset(test, [0, 0, 0]).await
    }

    #[allow(dead_code)]
    pub async fn run_test_with_offset(&mut self, test: &TestSpec, offset: [i32; 3]) -> Result<TestResult> {
        println!("\n{} {}", "Running test:".cyan().bold(), test.name.bold());
        if let Some(desc) = &test.description {
            println!("  {}", desc.dimmed());
        }

        let max_tick = test.max_tick();
        println!("  Timeline: {} ticks\n", max_tick);

        // Clean up test area before test
        let region = test.cleanup_region();
        let world_min = self.apply_offset(region[0], offset);
        let world_max = self.apply_offset(region[1], offset);
        println!("  {} Cleaning test area...", "→".blue());
        let cmd = format!(
            "fill {} {} {} {} {} {} air",
            world_min[0], world_min[1], world_min[2],
            world_max[0], world_max[1], world_max[2]
        );
        self.bot.send_command(&cmd).await?;
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

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
                    match self.execute_action(current_tick, entry, *value_idx, offset).await {
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
        let region = test.cleanup_region();
        let world_min = self.apply_offset(region[0], offset);
        let world_max = self.apply_offset(region[1], offset);
        println!("\n  {} Cleaning up test area...", "→".blue());
        let cmd = format!(
            "fill {} {} {} {} {} {} air",
            world_min[0], world_min[1], world_min[2],
            world_max[0], world_max[1], world_max[2]
        );
        self.bot.send_command(&cmd).await?;
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

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

    async fn execute_action(&mut self, tick: u32, entry: &TimelineEntry, value_idx: usize, offset: [i32; 3]) -> Result<bool> {
        match &entry.action_type {
            ActionType::Place { pos, block } => {
                let world_pos = self.apply_offset(*pos, offset);
                let cmd = format!("setblock {} {} {} {}", world_pos[0], world_pos[1], world_pos[2], block);
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
                    let world_pos = self.apply_offset(placement.pos, offset);
                    let cmd = format!(
                        "setblock {} {} {} {}",
                        world_pos[0], world_pos[1], world_pos[2], placement.block
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
                let world_min = self.apply_offset(region[0], offset);
                let world_max = self.apply_offset(region[1], offset);
                let cmd = format!(
                    "fill {} {} {} {} {} {} {}",
                    world_min[0], world_min[1], world_min[2],
                    world_max[0], world_max[1], world_max[2],
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
                let world_pos = self.apply_offset(*pos, offset);
                let cmd = format!("setblock {} {} {} air", world_pos[0], world_pos[1], world_pos[2]);
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
                for check in checks {
                    let world_pos = self.apply_offset(check.pos, offset);

                    // Poll with retries: 10 attempts, 50ms apart = up to 500ms total
                    // This handles timing issues in CI environments
                    let actual_block = self.poll_block_with_retry(world_pos, &check.is, 10, 50).await?;

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
                let world_pos = self.apply_offset(*pos, offset);
                let expected_value = &values[value_idx];

                // Poll with retries: 10 attempts, 50ms apart = up to 500ms total
                // This handles timing issues in CI environments
                let actual_value = self.poll_block_state_with_retry(world_pos, state, 10, 50).await?;

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
    #[allow(dead_code)]
    pub passed: usize,
    #[allow(dead_code)]
    pub failed: usize,
    pub success: bool,
}
