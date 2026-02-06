# lancer-apktool

A from-scratch **Rust reimplementation of [apktool](https://github.com/iBotPeaches/Apktool)** with a native desktop GUI — part of the Lancer bug-bounty toolkit. Decode, edit, rebuild, and sign Android APKs with no JVM required: a single static binary.

🚧 **In active development.**

## What it does

Given an `.apk`, `lancer-apktool` decodes binary `AndroidManifest.xml` (AXML) and `resources.arsc` into readable XML, disassembles `classes*.dex` into per-class `.smali`, and extracts assets/libs — then reverses the whole pipeline back into a fresh, signed APK. It is the round-trip tool that powers the mobile pentest loop (SSL-pin bypass, Frida-gadget injection, `debuggable` flips, network-security-config rewrites).

## Workspace layout

```
crates/
  lancer-apktool/   # CLI + native egui GUI (default binary) + lancer-apktool-cli companion
  apktool-core/     # orchestration: decode/build pipelines, framework cache, diff, patches
  smali/            # DEX <-> Smali assembler/disassembler
  axml/             # binary AndroidManifest.xml codec
  arsc/             # resources.arsc codec
  apksig/           # APK signing v1/v2/v3/v4
  aapt2-bridge/     # AAPT2 wrapper (and future pure-Rust port)
```

## CLI

```
lancer-apktool <SUBCOMMAND>

  decode  (d)                 decode an APK into an editable project tree
  build   (b)                 build an APK from a decoded project
  install-framework (if)      ingest a framework APK into the cache
  clean-frameworks  (cf)      wipe the framework cache (alias: empty-framework-dir/efd)
  publicize-resources         flip every resource's <public> flag
  sign                        native v1/v2/v3/v4 signing  (Lancer addition)
  diff                        structural APK diff (smali AST / manifest / resources)
  patch --preset <name>       pin-bypass | frida-gadget | debuggable |
                              webview-debug | nsc-bypass | proxy-ca
  self-test                   run the upstream parity matrix
```

Running `lancer-apktool` with no subcommand opens the native GUI. Pass `--cli`, or use the `lancer-apktool-cli` companion binary, for headless use.

## Build

### Windows
```
build.bat
```
Output: `build\lancer-apktool.exe`. For a packaged dist build: `build-windows.bat`.

### Linux / macOS
```bash
./build-linux.sh
```
Output: `build/lancer-apktool`.

### Requirements
- [Rust / Cargo](https://rustup.rs)
- (Windows) Visual C++ Build Tools 2019/2022
- Optional: a bundled/system `aapt2` binary (or set `AAPT2_BIN`) for resource recompilation

## License

MIT.
