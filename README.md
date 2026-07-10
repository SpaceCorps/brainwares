# Brainwares 🧠📦

> **Obsidian-style cross-referenced markdown memory storage meets Promptware.** Built for self-evolving AI agent applications and automated coding systems.

Brainwares (`bw`) is a cross-platform, dependency-free CLI tool written in Rust. It helps AI agents (like Claude Code) maintain a local, compounding knowledge graph of memories while tracking references to actual source code files. When referenced code changes, the system flags the associated memories as `OUTDATED`, signaling that the agent needs to reflect and update its knowledge base.

---

## Key Features

- 🔗 **Obsidian-Style Wiki-Links**: Link memories together using `[[Wiki-Links]]`.
- 🔄 **Backlinks Indexer**: Auto-resolves which memory pages link to a given note.
- ⚡ **Codebase Hash Tracking**: Map markdown memories to code file paths. The system verifies if code changes have made your memories outdated.
- 🤖 **Promptware Compiler**: Compiles prompt templates (`programs/`) along with relevant workspace memory and execution args into a single payload for agent runs.
- 🧹 **Vault Shaking**: Automatically finds dead links, orphan pages, and cleans up empty log files.
- 🦀 **Pure Rust & Cross-Platform**: No PowerShell required; runs natively on macOS, Linux, and Windows.

---

## Directory Structure

When initialized, Brainwares creates a `.brainwares/` vault directory:

```text
.brainwares/
  config.json             # Configuration settings (ignore patterns, paths)
  memories/               # Obsidian-style markdown pages
    index.md              # Entry point note
    auth-flow.md          # Memory documentation page
  programs/               # Promptware templates (agent instructions)
    refactor.md
  logs/                   # Execution history logs
```

---

## Markdown Memory Format

Brainwares notes are standard Markdown files with YAML frontmatter containing metadata and code references:

```markdown
---
title: Auth Flow
references:
  - path: "src/auth.rs"
    hash: "a1c2d3e4f5..."
tags: [auth, security]
last_updated: 2026-07-10T11:47:30Z
---

# Auth Flow

This page documents the authentication mechanics implemented in [[src/auth.rs]].
```

---

## Installation

### Prerequisites
- Rust and Cargo installed.

### Build and Install
Clone the repository and compile the release binary:

```bash
git clone https://github.com/SpaceCorps/brainwares.git
cd brainwares
cargo build --release
```

Move the compiled binary `target/release/brainwares` (or `bw`) into your system's `PATH`.

---

## CLI Reference

| Command | Arguments | Description |
| :--- | :--- | :--- |
| `bw init` | None | Initialize `.brainwares/` vault in the current directory. |
| `bw status` | None | Verify hashes of all referenced code files, check broken wiki-links, and list orphan files. |
| `bw add` | `<name> [--tags tags] [--title title] [--global]` | Create a new memory note (add `--global` / `-g` for user-wide vault). |
| `bw link` | `<memory_name> <code_path>` | Link a code file to a memory note (adds file and its current hash to frontmatter). |
| `bw update` | `<memory_name> [code_path]` | Recalculate and update stored hash(es) to match current filesystem state (re-verifies memories). |
| `bw shake` | None | Report dead links, prune orphan notes, and delete empty log files. |
| `bw query` | `<term>` | Search memories by title, tag, or content and prints matching snippets and backlinks. |
| `bw read` | `<memory_name>` | Renders memory page, active references status, and backlinks. |
| `bw compile` | `<program> [args...]` | Compiles a Promptware prompt with Firmware + Program + Memory + args. |
| `bw integrate` | None | Configure global Antigravity coding agent custom rules to natively interact with brainwares. |
| `bw doctor` | None | Audit the system installation (checks PATH, global agent rules, and local workspace). |

---

## Agent Usage & Workflow

For automated coding systems, the self-evolving reflection loop runs as follows:

```text
Step 1: Agent runs:  bw compile refactor --file src/main.rs
                      (Receives consolidated instructions and active memories)
                      
Step 2: Agent edits: src/main.rs
                      (This causes the hash of src/main.rs to change)
                      
Step 3: bw status    --> Reports "Memory 'auth-flow' has [OUTDATED CODE] src/main.rs"
                      
Step 4: Agent reads 'auth-flow.md', makes updates to match new src/main.rs code,
        then runs:   bw update auth-flow
                      (Memory hashes are now synchronized and status is clean again!)
```
