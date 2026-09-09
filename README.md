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

## Supported Commands
    run                 - Begin the debugging process
    b / break           - Pause program execution at a specific point
    clear               - Clear an existing breakpoint
    e / enable          - Enable an existing breakpoint
    dis / disable       - Disable an existing breakpoint
    d / delete          - Delete an existing breakpoint
    c / cont / continue - Resume program execution
    n / next            - Step over the next line of code
    si / stepi          - Execute the current instructtion
    s / step            - Step into the next line of code
    f / fin / finish    - Step out of the current function
    p / print           - Print the specified variable within the current scope
    i / info            - Display informating about the running process
    bt / backtrace      - Display the current stack trace
    l / ls / list       - Display the surrounding source code around current location
    q / quit            - Exit the debugger

- Refer to help for more informating on each entry
  Eg. help b or help break will give more detailed information on the command

## Contributing
See `CONTRIBUTING` for more information.

## License

Distributed under the MIT License. See `LICENSE` for more information.
