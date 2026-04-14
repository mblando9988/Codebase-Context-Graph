# Codebase Context Graph

**A native tool that scans local codebases and builds a queryable code graph for AI coding agents.**

Parses source files with Tree-sitter, stores the graph in SQLite/JSON, and exposes 14 MCP tools over stdio so AI agents can query code structure without re-reading everything.

---

## Download

### macOS (Apple Silicon)

**Option 1: Direct Download**

Go to [Releases](https://github.com/mblando9988/Codebase-Context-Graph/releases/latest) and download `codebase-context-graph-macos.tar.gz`:

```bash
curl -L https://github.com/mblando9988/Codebase-Context-Graph/releases/latest/download/codebase-context-graph-macos.tar.gz | tar -xz
cd codebase-context-graph-macos
./codebase-context-graph index --project-root /path/to/your/project
```

**Option 2: Desktop App**

Extract the download and open `Codebase Context Graph.app` for a GUI interface.

---

## Quick Start

```bash
# Initialize project config
./codebase-context-graph init --project-root /path/to/project

# Build the code graph
./codebase-context-graph index --project-root /path/to/project

# Start MCP server for AI agents
./codebase-context-graph serve --project-root /path/to/project
```

---

## Commands

| Command | Description |
|---------|-------------|
| `init` | Create `.codebase-context/config.json` |
| `index` | Build the code graph (standard mode) |
| `index --analysis-mode advanced` | Build full CPG (AST/CFG/DFG) |
| `smoke` | Quick validation test |
| `serve` | Start MCP stdio server |
| `watch` | Reindex on file changes |

---

## MCP Integration

Add to your Claude Code or IDE MCP configuration:

```json
{
  "mcpServers": {
    "codebase-context": {
      "command": "/absolute/path/to/codebase-context-graph",
      "args": ["serve", "--project-root", "/path/to/your/project"]
    }
  }
}
```

---

## What Gets Created

```
your-project/
└── .codebase-context/
    ├── config.json      # Project config
    ├── graph.db         # SQLite code graph
    └── graph.json       # JSON-LD export
```

---

## MCP Tools (14 Total)

| Tool | Description |
|------|-------------|
| `get_overview` | High-level stats: files, modules, functions |
| `get_module_map` | Module dependency map |
| `get_file_structure` | Symbols in a file |
| `trace_call_path` | Call paths between functions |
| `impact_analysis` | Impact of changing a node |
| `find_dependencies` | Upstream/downstream deps |
| `find_hubs` | Highly connected nodes |
| `search_symbols` | Search by name pattern |
| `get_data_flow` | Variable value flow |
| `get_community` | Nodes in a community |
| `find_entry_points` | App entry points |
| `get_cross_cutting` | Cross-cutting concerns |
| `diff_impact` | Impact of file changes |
| `get_node_detail` | Full node metadata |

---

## Supported Languages

**Tree-sitter parsers:**
- JavaScript / JSX
- TypeScript / TSX
- Python
- Bash
- Rust

Regex fallback for config/env heuristic discovery.

---

## Architecture

- **Standard index**: Parse tree → Symbol graph with CALLS/IMPORTS/DEPENDS_ON edges
- **Advanced index**: Adds AST cross-references, control flow (CFG), data flow (DFG)
- **MCP over stdio**: Returns TOON text or JSON-LD for token efficiency
- **Community detection**: Louvain algorithm on module dependency graph

---

## Building from Source

```bash
cargo build --release
./target/release/codebase-context-graph index --project-root /path/to/project
```

---

## License

MIT