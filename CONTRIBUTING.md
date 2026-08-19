# Contributing to imo

First off, thank you for taking your time to contribute! 

This document outlines the codebase architecture, development workflows, and guidelines to help you get started safely and quickly.

## Development Workflow

### 1. Multi-Platform Compilation Rules
Because platform-specific libraries like `nix` do not compile cleanly with Linux features on macOS or Windows, **never import platform-specific crates or types inside shared code**. 

*   **Isolate Native Logic**: Restrict `nix` calls and low-level system hooks entirely to `linux.rs` modules guarded by `#[cfg(target_os = "linux")]`.
*   **Provide Fallback Stubs**: Every item exposed by a Linux module must have a matching dummy stub inside a `#[cfg(not(target_os = "linux"))]` block to satisfy non-Linux targets during compilation checks.

## Pull Request Guidelines

1. **Keep PRs Focused**: Isolate changes to one feature at a time
2. **Never Break Cross-Platform Checks**: Ensure that your changes allow `cargo check` to run seamlessly on macOS and Windows by maintaining the fallback stubs.
3. **Clean Build Status**: Clean up any unused imports or unneeded mutable tags (`let mut`) before pushing. PRs must compile with zero errors and zero warnings.

