# Contributing to imo

First off, thank you for taking your time to contribute! 

This document outlines the codebase architecture, development workflows, and guidelines to help you get started safely and quickly.

## Codebase Architecture
``` text 
src/
├── main.rs            # Entry point; parses arguments and initializes sessions
├── linux.rs           # Core OS debug orchestration loop
├── helpers/           # Common utilities and parsing helpers 
│   ├── mod.rs         # imo interactive cli currently resides here
│   └── dwarf.rs       # DWARF structural parsing used for caching binary data
├── interface/         # OS specific implementations
│   ├── mod.rs         # Wrapper over user_regs_struct to display in a user friendly way
│   └── linux.rs       # Interface logic specific to Linux execution (such as writing and deleting INT3 instruction)
├── session/           # Core debugging state engine
│   ├── mod.rs         # The platform-independent DebugSession data structures (Main cache for the program)
│   └── linux.rs       # Low-level ptrace commands
└── test/              # Integration test suites
    ├── mod.rs         # Test root manager
    └── linux/         # Linux-specific runtime tests (Compile these with -g flag with gcc and -02 for inline_test to get desired results)
        ├── debug.rs   
        ├── mod.rs     
        ├── multiple/      # Multi-file C test targets (main.c, worker_a.c) ( meant to simulate tasks with mutiple files per line)
        └── running_task/  # Basic task ( meant to simulate a task with long simulation)
```

## Development Workflow

### 1. Multi-Platform Compilation Rules
Because platform-specific libraries like `nix` do not compile cleanly with Linux features on macOS or Windows, **never import platform-specific crates or types inside shared code**. 

*   **Isolate Native Logic**: Restrict `nix` calls and low-level system hooks entirely to `linux.rs` modules guarded by `#[cfg(target_os = "linux")]`.
*   **Provide Fallback Stubs**: Every item exposed by a Linux module must have a matching dummy stub inside a `#[cfg(not(target_os = "linux"))]` block to satisfy non-Linux targets during compilation checks.

### 2. Testing and Fixtures
The `src/test/linux/` directory contains active C source files and compiled test binaries used to validate the tracer engine.
*   When adding engine features, update or add a test target inside the appropriate folder.
*   Run the test suites to ensure components continue to behave as expected:

```bash
# Run all tests
cargo test

# Run tests with output streaming enabled
cargo test -- --nocapture
```

## Pull Request Guidelines

1. **Keep PRs Focused**: Isolate changes to one feature at a time
2. **Never Break Cross-Platform Checks**: Ensure that your changes allow `cargo check` to run seamlessly on macOS and Windows by maintaining the fallback stubs.
3. **Clean Build Status**: Clean up any unused imports or unneeded mutable tags (`let mut`) before pushing. PRs must compile with zero errors and zero warnings.

---

## Thank You!

Building a debugger from scratch is an intricate challenge. Every contribution, bug report, and code review helps make `imo` a more robust tool for the Rust ecosystem. Your support is deeply appreciated!

