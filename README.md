# Codebase Context Graph

Builds a queryable graph of a codebase from **compiler-grade semantic indexes** and
saves it to SQLite and JSON. It does not guess structure from syntax: it runs the
reference indexer for each language (rust-analyzer, the TypeScript compiler, Pyright)
and turns their resolved definitions and references into a graph of who defines what,
who calls whom and what depends on what. A small query server then lets another
program ask about that structure without reading every file again.

Written in Rust. Comes with a command-line tool and a small desktop app.

## Get started

The project is used straight from this repository; there are no prebuilt downloads. You need
[Rust](https://rustup.rs), plus the indexers for the languages you want (see below).

```bash
cd rust-cli
cargo build --release
./target/release/codebase-context-graph-gui    # the desktop app
```

The command line is described under [Use](#use).

## How it works

```
source files ──▶ one SCIP indexer per language ──▶ .scip index ──▶ graph.db + graph.json
                 rust-analyzer · scip-typescript · scip-python
```

[SCIP](https://github.com/scip-code/scip) is the open index format Sourcegraph uses for
precise code navigation. An indexer runs the real compiler front end, so every
reference is already resolved: with two classes that each have a `render()` method,
a call `chart.render()` points at `Chart#render()`, never at "something called render".
A syntax tree alone cannot tell them apart. This tool only reshapes that data.

## Install the indexers

The indexers are separate programs. `codebase-context-graph doctor` shows which are installed.

| Language | Indexer | Install |
|----------|---------|---------|
| Rust | [rust-analyzer](https://rust-analyzer.github.io) | `rustup component add rust-analyzer` |
| TypeScript, JavaScript | [scip-typescript](https://github.com/sourcegraph/scip-typescript) | `npm install -g @sourcegraph/scip-typescript` |
| Python | [scip-python](https://github.com/sourcegraph/scip-python) (Pyright) | `npm install -g @sourcegraph/scip-python` |

Tested with rust-analyzer 1.97, scip-typescript 0.4.0, scip-python 0.6.6 and Node 22.
Rust projects need a `Cargo.toml`; rust-analyzer builds with the project's normal
`target/` directory, like your editor does. Indexing this repo's own source takes about 40 s.

A file whose indexer is missing or failed is still in the graph but flagged as
uncovered, and `index` says so. If no indexer produced anything at all, it exits with an error.

## Use

After the build, the command-line tool is `rust-cli/target/release/codebase-context-graph`.
The examples call it `codebase-context-graph`: put that folder on your `PATH` or use the
full path.

```bash
codebase-context-graph doctor --project-root /path/to/project   # which indexers are installed
codebase-context-graph init   --project-root /path/to/project   # writes .codebase-context/config.json (keeps an existing one)
codebase-context-graph index  --project-root /path/to/project   # runs the indexers and builds the graph
codebase-context-graph smoke  --project-root /path/to/project   # checks the database and prints counts
codebase-context-graph serve  --project-root /path/to/project   # starts the query server
```

`index --scip some.scip` also ingests an index you built yourself (repeatable).
That works for any tool that writes SCIP, even without the indexers above installed.

Everything it writes goes in one folder inside your project:

```
.codebase-context/
├── config.json   # ignore patterns, timeouts, custom indexers
├── graph.db      # SQLite
├── graph.json    # the same data as JSON
└── scip/         # raw .scip indexes, indexer logs, generated tsconfig
```

The indexers do not write into your project, with one exception: rust-analyzer builds
with cargo, which creates `target/` if it is not there yet.

## Desktop app

`codebase-context-graph-gui` is a small window around the command-line tool, so both run
the same engine. It starts the `codebase-context-graph` binary that sits next to it, so run
it from `rust-cli/target/release/`. Choose a project folder and press **Index project**. The
window shows the indexer output and, when it finishes, the node and edge counts and how many
files had semantic data; **Open results folder** opens `.codebase-context/`. **Check
indexers** runs `doctor`, and **Stop** ends a run, including the indexers it started.

If the app is started without your shell's `PATH` (from Finder or the Dock, for example), it
also looks for the indexers in `~/.cargo/bin`, `~/.local/bin`, `~/.volta/bin`,
`~/.npm-global/bin`, `/opt/homebrew/bin` and `/usr/local/bin`. Put an absolute path in
`command` (see Configuration) if yours live elsewhere.

## Languages

| Language | Covered | Notes |
|----------|---------|-------|
| Rust | yes | rust-analyzer, from the top-most `Cargo.toml` |
| TypeScript, JavaScript | yes | The project's own `tsconfig.json`/`jsconfig.json` if there is one; otherwise a generated config that includes `.ts`, `.tsx`, `.js` and `.jsx` files |
| Python | yes | |
| Bash | no | There is no semantic indexer for shell, so it is no longer parsed |
| Other SCIP languages | untested | Add an entry under `indexers` in `config.json`, or pass `--scip` |

## What is in the graph

Nodes:

| Type | Meaning |
|------|---------|
| `FILE` | A source file (`covered: false` if no index covered it) |
| `MODULE` | A directory |
| `FUNCTION`, `METHOD`, `STRUCT`, `ENUM`, `TRAIT`, `INTERFACE`, `CLASS`, `TYPE`, `TYPE_ALIAS`, `FIELD`, `VARIABLE`, `CONSTANT`, `ENUM_MEMBER`, `MACRO`, `NAMESPACE` | Definitions. TypeScript and Python indexers do not say what kind a type is, so those show as `TYPE` |
| `EXTERNAL` | A dependency package (`std`, `serde`, `python-stdlib`, ...) |

Edges:

| Type | Meaning |
|------|---------|
| `CONTAINS` | directory → file, type → its members |
| `DEFINES` | file → its top-level definitions |
| `CALLS` | a reference to a function, method or macro made from inside another definition |
| `REFERENCES` | any other resolved reference: types, fields, imports, module-level code |
| `IMPLEMENTS` | from the indexer's relationship data (scip-typescript provides it) |
| `USES` | definition or file → `EXTERNAL` package |
| `DEPENDS_ON` | file → file and directory → directory, derived from the edges above |

Node ids are stable and readable, `{package manager}:{package}:{symbol}`, for example
`cargo:codebase-context-graph:indexer/index_project().`. The package version is not part of
the id. Metadata carries the hover signature, doc comment, type-qualified name
(`Chart.render`) and `fanIn`/`fanOut`/`hubScore` (how many distinct nodes depend on it).

Two details worth knowing:

* SCIP cannot tell an import from a call at module level, so references outside any
  definition are `REFERENCES`, and `CALLS` only means a call-like reference from inside
  another definition. Passing a function as a value counts too.
* When one symbol is defined more than once (two binaries that each have `main`), the first
  definition in path order is the node; the others get a `~2` suffix and `duplicateOf`.

## Query server

`serve` reads one JSON request per line on stdin and writes one JSON answer per line on
stdout. Most answers hold a small text table in `content`.

```json
{"method": "search_symbols", "params": {"query": "render", "limit": 20}}
```

| Method | Params | Returns |
|--------|--------|---------|
| `get_overview` | | Counts, covered files, and what each indexer did |
| `get_module_map` | | Every directory, its file count and the directories it depends on |
| `get_file_structure` | `file_path` | Definitions in one file with line ranges and signatures |
| `search_symbols` | `query`, `limit`, `type` | Definitions whose name or qualified name contains `query`: exact name matches first, then the most depended-on |
| `get_node_detail` | `node_id` | One node with its metadata and its incoming and outgoing edges |
| `find_hubs` | `limit`, `type` | Nodes ranked by how many others depend on them |

## Configuration

`config.json` is created on `init` and never overwritten by it.

```json
{
  "version": "2.0",
  "project_name": "my-project",
  "ignore_patterns": ["node_modules/", "target/", "*.min.js", "legacy/"],
  "respect_gitignore": true,
  "indexer_timeout_secs": 1800,
  "indexers": [
    {
      "name": "my-indexer",
      "languages": ["go"],
      "extensions": ["go"],
      "markers": ["go.mod"],
      "command": ["my-indexer", "--output", "{output}"],
      "install_hint": "how to install it"
    }
  ]
}
```

`ignore_patterns` use gitignore syntax. Only dependency, build-output and cache directories
are ignored by default; directories such as `db/` or `data/` are indexed. An `indexers`
entry named like a built-in one replaces it (`"enabled": false` turns it off); any other
name adds a new indexer. In `command`, `{output}` is the `.scip` file to write and
`{project_name}` the project name; it runs in the project root it was matched to.

## Known limits

- The query server uses its own line format, not the MCP protocol.
- The desktop app starts `index` and `doctor` and shows their output. It does not display the
  graph; read `graph.db` or `graph.json`, or use `serve`.
- `--analysis-mode` is accepted and ignored. There is one mode, and it is always semantic.
- Every `index` re-runs the indexers and rebuilds everything. `watch` runs one index and exits.
- One run per top-most project root (`Cargo.toml`, `tsconfig.json`, `package.json`,
  `pyproject.toml`, ...). A monorepo whose root manifest does not cover all packages needs
  extra `indexers` entries or `--scip` files.
- An indexer that crashes covers none of its language's files in that root. The crash and
  the log path are shown by `index` and stored in `graph.db` (`index_runs`).
- Test code is part of the graph, and standard-library packages show up as `EXTERNAL`.
- scip-typescript puts the absolute project path into symbol ids when there is no
  `package.json` above the project.
- The graph is built in memory; it is not tuned for very large repositories.

## Tests

```bash
cd rust-cli
cargo test --release
```

## License

MIT
