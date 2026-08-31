# imo

A Rust-native debugger designed to natively support and smoothly visualize Rust-specific types like `Option`, `Result`, and standard library collections.

> ⚠️ **Project Status: Early Development**. Core features are still in progress. APIs and functionality are subject to frequent changes.

## Getting Started

### Prerequisites

*   **Supported Operating Systems:** Linux (x86_64 / AArch64) — *macOS and Windows support are planned.*
*   **System Dependencies:** `ptrace` capabilities.
*   **Toolchain:** Rust (Stable) 

### Installation

1. **Clone the repository:**
   ```bash
   git clone https://github.com/tommy278/imo.git
   ```

2. **Build the binary:**
   ```bash
   cd imo
   cargo build --release
   ```

## Usage

To start a debugging session, provide the target binary path as the primary argument to `imo`:

```bash
cargo run /path/to/target_binary
```

## Contributing
See `CONTRIBUTING` for more information.

## License

Distributed under the MIT License. See `LICENSE` for more information.
