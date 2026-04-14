# Codebase Context Graph

A native Rust tool that scans any local codebase, parses source files with Tree-sitter, and builds a queryable code graph stored in SQLite and JSON. It exposes 14 MCP tools over stdio so AI coding agents can query your codebase's structure without re-reading everything on every request.

## Download & Run

**macOS Apple Silicon:**
```bash
curl -L https://github.com/mblando9988/Codebase-Context-Graph/releases/latest/download/codebase-context-graph-macos-aarch64.tar.gz | tar -xz
cd codebase-context-graph-macos-aarch64
./codebase-context-graph index --project-root /path/to/your/project
```

Or double-click `Launch Codebase Context Graph.app` for the desktop UI.

## Commands

| Command | Description |
|---------|-------------|
| `init` | Create per-project config |
| `index` | Build the code graph (standard mode) |
| `index --analysis-mode advanced` | Build the full CPG (AST/CFG/DFG) |
| `smoke` | Quick sanity check |
| `serve` | Start MCP stdio server for AI agents |
| `watch` | Reindex on file changes |

## What Gets Created

- `.codebase-context/config.json` — project config and language settings
- `.codebase-context/graph.db` — full code graph in SQLite
- `.codebase-context/graph.json` — same graph in JSON-LD format

## Supported Languages

JavaScript, TypeScript, Python, Bash, Rust — via Tree-sitter parsers. Regex fallback for config/env heuristics.

## Architecture

- `CALLS` edges cover direct static call sites (sparse for dynamic/runtime dispatch)
- Standard index: parse tree → symbol graph
- Advanced index: adds AST cross-references, control flow, and data flow edges
- MCP tools return TOON text or JSON-LD for token efficiency
