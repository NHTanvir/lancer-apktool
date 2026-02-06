# CLAUDE.md

Guidance for Claude Code (and humans) working in this repository.

## What this is

`lancer-apktool` — A from-scratch Rust reimplementation of apktool with a native desktop GUI, part of the Lancer 2026 bug-bounty toolkit. It decodes binary `AndroidManifest.xml` (AXML) and `resources.arsc` into readable XML, disassembles DEX bytecode into Smali, and reverses the entire pipeline back into a fresh signed APK — with no JVM required. It powers the mobile pentest loop, enabling SSL-pin bypass, Frida-gadget injection, `debuggable` flips, network-security-config rewrites, and structural APK diffs.

## Architecture

This is a Cargo workspace. Key crates and modules:

- `crates/lancer-apktool/src/main.rs` — binary entry point; launches GUI by default, `--cli` flag for headless mode
- `crates/lancer-apktool/src/lib.rs` — library entry point for the main crate
- `crates/lancer-apktool/src/cli.rs` — CLI surface: subcommands (`decode`, `build`, `sign`, `diff`, `patch`, `self-test`, etc.)
- `crates/lancer-apktool/src/gui/mod.rs` — egui desktop GUI module root
- `crates/lancer-apktool/src/gui/app.rs` — egui application state and event loop
- `crates/lancer-apktool/src/gui/server.rs` — background server bridging GUI to core pipelines
- `crates/lancer-apktool/src/bin/cli.rs` — `lancer-apktool-cli` companion headless binary
- `crates/apktool-core/src/lib.rs` — orchestration: decode/build pipelines, framework cache, diff, patches; sub-modules in `decode/`, `build/`, `patches/`, `diff/`
- `crates/smali/src/lib.rs` — DEX ↔ Smali assembler/disassembler (`asm.rs`, `disasm.rs`, `dex.rs`, `opcodes.rs`)
- `crates/axml/src/lib.rs` — binary `AndroidManifest.xml` codec (`reader.rs`, `document.rs`, `string_pool.rs`, `xml_parse.rs`)
- `crates/arsc/src/lib.rs` — `resources.arsc` codec (`table.rs`, `config.rs`)
- `crates/apksig/src/lib.rs` — APK signing v1/v2/v3/v4 (`v1.rs`–`v4.rs`, `zip_sections.rs`)
- `crates/aapt2-bridge/` — AAPT2 wrapper; set `AAPT2_BIN` env var or provide a bundled binary

## Conventions

- Error propagation uses `anyhow::Result`; no `unwrap`/`expect` on fallible I/O.
- GUI is gated behind the default `gui` feature; `--no-default-features` builds a headless CLI binary.
- Library code lives in `src/lib.rs`; binary entry point in `src/main.rs`.
- All crates share workspace-level dependency versions declared in the root `Cargo.toml` — add new deps to `[workspace.dependencies]` first, then reference with `{ workspace = true }`.
- Tracing (`tracing` crate) is used for structured logging; prefer `tracing::info!` / `tracing::error!` over `eprintln!`.
- The `frameworks/` directory is a local framework cache (like the original apktool's `~/.apktool/frameworks/`) and is gitignored; the `keystore/` directory holds signing keys and must not be committed.

## Build / test / run

```
build.bat                   # Windows → build\lancer-apktool.exe
build-windows.bat           # Windows packaged dist build
./build-linux.sh            # Linux/macOS → build/lancer-apktool
cargo build --release       # standard release build
cargo test                  # unit + integration tests
```

Set `AAPT2_BIN` environment variable to override the default `aapt2` binary location.

## Repo notes

- `plan.md` is local-only and must NOT be committed. It is gitignored; if ever tracked, run `git rm --cached plan.md`.
- `.claude/` and `target/`/`build/`/`dist/` are local-only and gitignored.
- `frameworks/` and `keystore/` are local runtime directories; do not commit their contents.
