# imo

A Rust-native debugger designed to natively support and smoothly visualize Rust-specific types like `Option`, `Result`, and standard library collections.

> **Imo** (ìmọ̀) is the Yoruba word for *knowledge*. This debugger is built to give you deeper knowledge of your running Rust code.

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

### Interactive Commands

Once the session initializes, you will enter the interactive `(imo)` prompt. The interface accepts short aliases and is completely case-insensitive.

#### 🚀 Execution Control
*   **`run`** | **`c`** | **`continue`**
    *   Starts target process execution or resumes it from the current breakpoint.

#### 📍 Managing Breakpoints
*   **`b <target>`** | **`break <target>`**
    *   Sets a new breakpoint. Supports two distinct lookup patterns:
        *   *By Line Only:* `break 12` (Prompts for specific file if the line number matches multiple source files).
        *   *By File & Line:* `break running_task:6` (Explicit isolation).
*   **`clear <target>`**
    *   Clears a breakpoint by location coordinates. Supports `clear 12` or `clear running_task:6`.

#### 🔢 Index-Based Breakpoint Mutations
Breakpoints are tracked using a **1-based index** system. You can toggle their runtime states instantly using their index numbers:
*   **`dis <index>`** | **`disable <index>`**
    *   Temporarily disables an active breakpoint (e.g., `disable 1`).
*   **`e <index>`** | **`enable <index>`**
    *   Re-enables a deactivated breakpoint (e.g., `enable 1`).
*   **`d <index>`** | **`delete <index>`**
    *   Permanently deletes a breakpoint configuration from the track matrix.

#### 🔍 Information
*   **`i b`** | **`info breakpoints`**
    *   Lists all registered breakpoints along with their matching tracking index and structural status metadata.
*   **`i reg`** | **`info reg`**
    *   Dumps all current CPU registers formatted cleanly with hex layout matrices, and decimal values.

#### 🛑 Session Termination
*   **`q`** | **`quit`**
    *   Gracefully stops tracing, terminates the target child process using a system `SIGKILL` wire signal, and closes the terminal loop.

### Example

```text
\$ cargo run -- src/test/linux/multiple/inline_test
(imo) b 10
Breakpoint 1 at 0x108B: file main.c, line 10

(imo) run
Starting Main
Utility executing inside worker 1

(imo) i reg
rax      0x22                34
rbx      0x0                 0
rip      0x555555555078      0x555555555078
eflags   0x246               [ PF ZF IF ]

(imo) q
Child process was killed by SIGKILL signal
```


## Roadmap

To track our development progress toward a stable release:
- [x] Basic process spawning and `ptrace` attachment
- [x] DWARF debugging information parsing
- [ ] Structural visualization for `Option<T>` and `Result<T, E>`
- [ ] Interactive CLI / TUI interface
- [ ] Expression evaluation

## Contributing
See `CONTRIBUTING` for more information.

## License

Distributed under the MIT License. See `LICENSE` for more information.

