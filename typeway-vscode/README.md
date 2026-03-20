# Typeway Migration Tool for VS Code

Convert between Axum and Typeway code directly in your editor.

## Prerequisites

Install the `typeway-migrate` CLI tool:

```sh
cargo install --path typeway-migrate
```

Or set the binary path in settings: `typeway.migrateBinaryPath`.

## Commands

| Command | Description |
|---------|-------------|
| `Typeway: Convert Axum -> Typeway` | Convert the current file from Axum to Typeway |
| `Typeway: Convert Typeway -> Axum` | Convert the current file from Typeway to Axum |
| `Typeway: Preview Axum -> Typeway Conversion` | Preview conversion in a side-by-side tab |
| `Typeway: Preview Typeway -> Axum Conversion` | Preview conversion in a side-by-side tab |
| `Typeway: Check Current File` | Analyze the file and report detected features |

## Usage

1. Open a Rust file containing Axum or Typeway code
2. Open the Command Palette (`Cmd+Shift+P` / `Ctrl+Shift+P`)
3. Type "Typeway" to see available commands
4. Or right-click in the editor for context menu options

## Features

- **Preview mode**: See the conversion result side-by-side before committing
- **Automatic backups**: `.bak` files created before modifying
- **Check command**: Shows detected endpoints, auth patterns, effects, and warnings
- **Context menu**: Right-click on any Rust file for quick access

## Settings

| Setting | Default | Description |
|---------|---------|-------------|
| `typeway.migrateBinaryPath` | `typeway-migrate` | Path to the CLI binary |
| `typeway.createBackups` | `true` | Create backup files before converting |

## How It Works

This extension is a thin wrapper around the `typeway-migrate` CLI tool. It does not reimplement any migration logic -- it shells out to the binary for all conversions and file analysis. This ensures the extension always matches the behavior of the CLI.

### Dry Run / Preview

The preview commands (`--dry-run`) run the conversion without modifying the original file and display the result in a new editor tab alongside the original. This lets you review the changes before applying them.

### Check

The check command analyzes a Rust file and reports detected Axum or Typeway patterns, endpoints, middleware usage, and any warnings about constructs that may need manual attention during migration.
