# Codebase Context Graph

Scans a local codebase, parses it with Tree-sitter, and saves what it finds
(files, modules, functions and classes, with line ranges) to SQLite and JSON.
A small query server then lets another program ask about that structure
without reading every file again.

Written in Rust. Comes with a command-line tool and a small desktop app.

## Download (macOS, Apple Silicon)

The v1.0.0 download is a different build from the source here: a Node.js
indexer and MCP server with a Python desktop app, started by a Rust launcher.
The Rust code in this repo is a rewrite that covers less so far (see "Not done
yet" below).

```bash
curl -L https://github.com/mblando9988/Codebase-Context-Graph/releases/latest/download/codebase-context-graph-macos-aarch64.tar.gz | tar -xz
cd codebase-context-graph-macos-aarch64
```

The folder also has `Codebase Context Graph.app` if you want the desktop app.
To use the Rust version, build from source (below).

## Use

```bash
./codebase-context-graph init  --project-root /path/to/project   # writes .codebase-context/config.json
./codebase-context-graph index --project-root /path/to/project   # scans and parses the project
./codebase-context-graph smoke --project-root /path/to/project   # checks the database and prints the node count
./codebase-context-graph serve --project-root /path/to/project   # starts the query server
```

Everything it writes goes in one folder inside your project:

```
.codebase-context/
├── config.json   # ignore patterns and settings
├── graph.db      # SQLite
└── graph.json    # the same data as JSON
```

## Languages

JavaScript, TypeScript, Python, Bash and Rust files are parsed with
Tree-sitter. So far only top-level functions and classes are recorded, and Rust
files are stored as files only.

## Query server

`serve` reads one JSON request per line on stdin and writes one JSON answer per
line on stdout. Most answers hold a small text table in `content`.

```json
{"method": "search_symbols", "params": {"query": "parse", "limit": 20}}
```

| Method | Params | Returns |
|--------|--------|---------|
| `get_overview` | | Counts of files, modules and functions |
| `get_module_map` | | Every module |
| `get_file_structure` | `file_path` | Symbols in one file with their line ranges |
| `search_symbols` | `query`, `limit` | Symbols whose name matches |
| `get_node_detail` | `node_id` | Everything stored for one node |
| `find_hubs` | `limit` | Nodes ranked by hub score |

## Not done yet

- Call and import edges. Right now the graph only links files to what they
  contain.
- Methods, exported and nested symbols, and Rust symbols.
- `find_hubs` returns no rows yet, and `search_symbols` applies its limit
  before matching the name, so it can miss symbols.
- `watch` runs one index and exits instead of watching for changes.
- `--analysis-mode` is accepted but doesn't add anything yet.
- The server uses its own line format, not the MCP protocol.

## Build from source

```bash
cd rust-cli
cargo build --release
./target/release/codebase-context-graph index --project-root /path/to/project
```

The desktop app builds as `codebase-context-graph-gui` in the same folder.

## License

MIT
