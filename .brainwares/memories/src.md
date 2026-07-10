---
title: Src
references:
- path: src/models.rs
  hash: 3e4520b4d8c1a579f27c3946709f540dfffe58a3a24b49835e59614d8ae5fb49
- path: src/commands.rs
  hash: 870e7d429fb4299f8a936f1236c81f0812bb1418d776c99685d3dbf4096e6f81
- path: src/main.rs
  hash: 3e87c84fb656903f530edde0333feb2fe304a433948615cd6c4ebca9f9e22abc
- path: src/parser.rs
  hash: ed47de0de466de22635ff16cd236dffadde6e4c568b513a8623a0d22d0968ae1
- path: src/hash.rs
  hash: 661c16a520b92112a4b6f260a20caba130677443ac9df0e09c605903d01e3c01
- path: src/vault.rs
  hash: 5431a229774e75ebfc8958de922d5f424d944cd8e9eb2928f24b2f0b994d42a0
- path: src/cli.rs
  hash: 30453e1a5f6f09e932c9032f36a80c2e4877f23d2298ad228b64e8d25d6ce77a
- path: src/engine.rs
  hash: ca88fa4d18bf76d5031aab210b8ceb5de58c90dffd879338a0aa990164daef5e
tags:
- folder
- index
last_updated: 2026-07-10T12:21:05.690843+00:00
---

# Src

Scaffolded memory page for the `src` directory.

## Core Files Reference Map

*   `models.rs`: Data structures and serialization schemas for config and memory frontmatter.
*   `commands.rs`: Core handlers for CLI subcommands including init, status, indexing, and UI visualization.
*   `main.rs`: Entry point parsing CLI arguments and routing subcommand handlers.
*   `parser.rs`: Obsidian-style Markdown frontmatter parser and wiki-link extractor.
*   `hash.rs`: File hashing helper using SHA-256 to detect out-of-date states.
*   `vault.rs`: Vault configuration loading, path helpers, and memory backlink resolver.
*   `cli.rs`: Command Line Interface structure definitions mapped via clap.
*   `engine.rs`: Diagnostic status check, validation rules, and context loader routines.
