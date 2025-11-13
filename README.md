# FlintMC

A Minecraft server testing framework written in Rust using Azalea. Tests are specified in JSON and executed deterministically using Minecraft's `/tick` command.

## Features

- **Timeline-based testing**: Actions are executed at specific game ticks for deterministic behavior
- **JSON test specification**: Write tests in simple JSON format
- **Block mechanics testing**: Test block states, properties, and interactions
- **Directory support**: Run single test files or entire directories of tests
- **Fast execution**: Uses `/tick freeze` and `/tick step` to skip empty ticks

## Requirements

- Rust 1.85+ (2024 edition)
- Minecraft server 1.21.5+
- Bot needs operator permissions on the server

## Installation

```bash
cargo build --release
```

## Usage

### Run a single test file:
```bash
cargo run -- example_tests/basic_placement.json --server localhost:25565
```

### Run all tests in a directory:
```bash
cargo run -- example_tests/ --server localhost:25565
```

### Run all tests recursively:
```bash
cargo run -- example_tests/ --server localhost:25565 --recursive
```

## Test Format

Each test is a JSON file with the following structure:

```json
{
  "flintVersion": "0.1",
  "name": "test_name",
  "description": "Optional description",
  "tags": ["tag1", "tag2"],
  "dependencies": ["optional_dependency1", "optional_dependency2"],
  "setup": {
    "cleanup": {
      "region": [[x1, y1, z1], [x2, y2, z2]]
    }
  },
  "timeline": [
    {
      "at": 0,
      "do": "place",
      "pos": [x, y, z],
      "block": "minecraft:block_id"
    },
    {
      "at": 1,
      "do": "assert",
      "checks": [
        {"pos": [x, y, z], "is": "minecraft:block_id"}
      ]
    }
  ]
}
```

The `setup.cleanup` field is optional. If specified, the framework will:
1. Fill the area with air **before** the test runs
2. Fill the area with air **after** the test completes

This ensures tests don't interfere with each other.

## Available Actions

### Block Operations

**place** - Place a single block
```json
{
  "at": 0,
  "do": "place",
  "pos": [x, y, z],
  "block": "minecraft:block_id"
}
```

**place_each** - Place multiple blocks
```json
{
  "at": 0,
  "do": "place_each",
  "blocks": [
    {"pos": [x1, y1, z1], "block": "minecraft:block_id"},
    {"pos": [x2, y2, z2], "block": "minecraft:block_id"}
  ]
}
```

**fill** - Fill a region with blocks
```json
{
  "at": 0,
  "do": "fill",
  "region": [[x1, y1, z1], [x2, y2, z2]],
  "with": "minecraft:block_id"
}
```

**remove** - Remove a block (replace with air)
```json
{
  "at": 0,
  "do": "remove",
  "pos": [x, y, z]
}
```

### Assertions

**assert** - Check block type(s) at position(s)
```json
{
  "at": 1,
  "do": "assert",
  "checks": [
    {"pos": [x, y, z], "is": "minecraft:block_id"}
  ]
}
```

**assert_state** - Check block property value(s)
```json
{
  "at": 1,
  "do": "assert_state",
  "pos": [x, y, z],
  "state": "property_name",
  "values": ["expected_value"]
}
```

For multiple ticks, use an array:
```json
{
  "at": [1, 2, 3],
  "do": "assert_state",
  "pos": [x, y, z],
  "state": "powered",
  "values": ["false", "true", "false"]
}
```

## Example Tests

See the `example_tests/` directory for examples:

- `basic_placement.json` - Simple block placement
- `fences/fence_connects_to_block.json` - Fence connection mechanics
- `fences/fence_to_fence.json` - Fence-to-fence connections
- `redstone/lever_basic.json` - Lever placement and state
- `water/water_source.json` - Water source block

## How It Works

1. Bot connects to server in spectator mode
2. Test timeline is constructed from JSON
3. Server time is frozen with `/tick freeze`
4. Actions are grouped by tick and executed
5. Between tick groups, `/tick step 1` advances time
6. Azalea tracks world state from server updates
7. Assertions verify expected block states
8. Results are collected and reported

## Architecture

```
src/
├── main.rs       - CLI and test runner
├── test_spec.rs  - JSON parsing and test specification
├── bot.rs        - Azalea bot controller
└── executor.rs   - Test execution and timeline management
```

## License

MIT
