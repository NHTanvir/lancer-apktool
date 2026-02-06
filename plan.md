# RESEARCH DOSSIER: APKTool

> Part of the Lancer ecosystem. A Rust clone of iBotPeaches/Apktool — APK decode + rebuild + smali toolchain. Build with Claude Code.
> Upstream: github.com/iBotPeaches/Apktool (Java, Apache-2.0, ~2.9.x → 2.10.x in 2025-2026). Ships as a fat JAR plus a wrapper script. No Rust equivalent in the ecosystem.

## Feature Parity Mandate

**`lancer-apktool` MUST ship with 100% feature parity to the upstream `apktool` tool — every flag, every subcommand, every output format, every probe / detector / module / payload / mode — plus a native Rust GUI on top.** This is not a "subset clone" — this is a full reimplementation in Rust with a GUI wrapper.

### Parity contract

- **Every CLI flag** documented in upstream `apktool --help` (and the long-form man page / docs) must work in `lancer-apktool --cli`.
- **Every output format** the upstream tool emits (text, JSON, XML, CSV, SARIF, custom formats) must round-trip through `lancer-apktool` byte-equivalent where the format allows.
- **Every probe / detector / module / category / payload class / built-in script** must be ported. None silently dropped.
- **Every supported target type / input format / protocol / file format / language** must work.
- **Every configuration option** (config files, env vars, registry / settings entries) must be honoured.
- **Companion binaries / sub-tools** in the upstream suite (if any) must each get their own binary in this workspace.

### GUI on top

Every feature reachable via CLI is also reachable from the GUI (see `## GUI (Lancer-standard pattern)` below). The GUI is not a subset — it is an alternate frontend over the same `core` library that the CLI uses. The `core` crate is GUI-agnostic.

### Verification

- The `## Granular feature catalog` section below enumerates everything that must be supported.
- Reference: `tests/upstream_parity/` will pin upstream flag combinations + canned outputs as parity tests. CI fails if any flag is unsupported or output diverges.
- A `lancer-apktool --self-test` flag runs the full parity matrix against a bundled reference target and prints pass/fail per upstream feature group.

### What is allowed to diverge

- **Performance** — Rust-native, async-first; we are allowed to be faster.
- **Memory footprint** — we are allowed to be smaller.
- **GUI** — entirely new (no upstream GUI to copy except where the upstream ships its own GUI like Zenmap / Wireshark UI / Ghidra GUI / jadx-gui — in which case our GUI must cover every panel the upstream GUI has, plus the Lancer-standard layout from `## GUI` below).
- **AI augmentation** — net-new (see `## AI augmentation ideas`).

### What is NOT allowed to diverge

- Any flag silently dropped, renamed, or behavior-changed compared to upstream.
- Any output format / record schema diverging in field names, types, or semantics.
- Any built-in script / detector / module not ported (mark explicitly with `[N/A — see note]` if genuinely impossible in Rust, e.g. NSE Lua scripts requiring features not in `mlua`).

## What it actually does

APKTool is the standard tool for **modifying** Android APKs. JADX is for **reading** (read-only decompilation to Java); APKTool is for **changing** (decode → edit → repack → resign → reinstall). It does the round-trip that no other open-source tool does cleanly. Given an `.apk` it: decodes `resources.arsc` into a `res/` tree of XML files; decodes binary `AndroidManifest.xml` (AXML) into text XML; disassembles `classes*.dex` into per-class `.smali` source files via JesusFreke's `smali`/`baksmali`; resolves resource references against a cached framework-res.apk; extracts assets, libs, raw resources; and after user edits, reverses the whole pipeline back into a fresh APK, signs it, and zipaligns it.

This is **the** APK-modification tool. The pentest loop — patch SSL pinning → repack → reinstall to bypass certificate pinning at runtime; inject a Frida gadget into `<application>` so a process can be instrumented without root; flip `android:debuggable="false"` to `"true"`; rewrite `network_security_config.xml` so a proxy CA is trusted; patch `WebView.setWebContentsDebuggingEnabled(false)` to `true` so a remote debugger can attach — all of these only work because APKTool round-trips smali and AXML losslessly. There is **no Rust equivalent**. We will build one (`lancer-apktool`).

## Internal architecture

**Workspace** (`Cargo.toml`):
```
[workspace]
members = ["lancer-apktool", "apktool-core", "smali", "axml", "arsc", "aapt2-bridge", "apksig"]
```

- **`lancer-apktool`** — CLI binary (clap 4.x). Parses subcommands (`d`/`decode`, `b`/`build`, `if`/`install-framework`, `empty-framework-dir`, `publicize-resources`, `diff`, `sign`). Dispatches to `apktool-core`.
- **`apktool-core`** — Orchestration: zip in/out, APK version detection, framework caching, decode/build pipelines, project YAML (`apktool.yml`) read/write.
- **`smali`** — DEX ↔ Smali assembler/disassembler. Port of JesusFreke's smali/baksmali. Must be round-trip safe (re-assemble a freshly disassembled DEX → byte-identical output, modulo string-pool ordering).
- **`axml`** — Binary `AndroidManifest.xml` encoder/decoder (the Android binary XML format with string pool + resource map + namespace stack).
- **`arsc`** — `resources.arsc` encoder/decoder (resource table chunks, type specs, config qualifier matrix).
- **`aapt2-bridge`** — Bare-minimum resource compiler. Initially wraps the Google `aapt2` binary behind a `feature = "native-aapt2"` flag; longer term we port enough to compile a `res/` tree into a fresh ARSC.
- **`apksig`** — APK signing: PKCS#7 v1 (JAR signing), v2/v3 (APK Signature Scheme block in the central directory), v4 (separate `.apk.idsig`). Uses `pkcs7`/`rsa`/`x509-cert`/`p256`/`ed25519`.

**Key modules in `apktool-core/src/`:**

- `decode/`
  - `mod.rs` — Top-level decode: open APK as zip, detect framework version (read `targetSdkVersion` from binary AndroidManifest), pull `resources.arsc`, dispatch resource decode + smali decode + manifest decode + asset extract.
  - `manifest.rs` — AXML → text XML. Resolves resource refs (`@android:string/foo` → readable string), preserves namespace declarations, handles the namespace stack correctly.
  - `resources.rs` — Walk `resources.arsc` chunks (`RES_TABLE_TYPE`, `RES_TABLE_PACKAGE_TYPE`, `RES_TABLE_TYPE_TYPE`, `RES_TABLE_TYPE_SPEC_TYPE`, `RES_STRING_POOL_TYPE`), emit `res/values/strings.xml`, `res/values-<qualifier>/*.xml`, `res/drawable/*`, `res/layout/*.xml` (each layout itself AXML → text), `res/mipmap-*`, etc.
  - `smali_decode.rs` — For each `classes*.dex` in the APK, invoke `smali::disassemble(dex)` → write `smali/`, `smali_classes2/`, `smali_classes3/`, ... per Android multi-DEX convention.
  - `library.rs` — Extract `lib/<abi>/*.so`, `assets/`, raw resources (`res/raw/`). Preserve compression flag (which files in the original APK were stored, which were deflated).
  - `apktool_yml.rs` — Write the `apktool.yml` project file: original SDK target, version code/name, compression bookkeeping (so rebuild can preserve `STORED` vs `DEFLATED` per entry), unknown files list.

- `build/`
  - `mod.rs` — Reverse pipeline: text XML → AXML, source res → ARSC (via `aapt2-bridge`), smali → DEX, zip everything per `apktool.yml`.
  - `signer.rs` — Sign APK: v1 (JAR-style: META-INF/MANIFEST.MF + META-INF/CERT.SF + META-INF/CERT.RSA), v2 (APK Signing Block), v3 (rotation), v4 (`.apk.idsig` for incremental install). Default to v1+v2+v3 with bundled debug keystore.
  - `zipalign.rs` — Pad uncompressed entries so their data starts at 4-byte boundaries; modern v2 also requires 4K-aligned native libs (`lib/*.so`) for page-zip optimizations.
  - `compression.rs` — Honor per-entry STORED/DEFLATED from `apktool.yml`. Some entries (`*.png`, `*.arsc` post Android 6) MUST be STORED.

- `framework/`
  - `cache.rs` — Maintain `~/.local/share/lancer-apktool/framework/<package>-<version>.apk`. Default ships with `android-1.apk` (frame-res for stock framework). User can install OEM framework APKs (Samsung, MIUI) via `install-framework`.
  - `install.rs` — Implements `install-framework` / `if` subcommand.

- `diff/` (Lancer extension)
  - `mod.rs` — Decode both APKs into temp dirs, compute structural diff: smali AST diff (not text diff — text diff is noisy because register numbers shift), manifest semantic diff (added permissions, intent filters, debuggable flag flip), resource diff (which strings changed, which drawables differ by hash). Output JSON or human-readable.

- `patches/` (Lancer extension)
  - `pin_bypass.rs` — Walk all smali, find every `checkServerTrusted`/`checkClientTrusted`/`verify` implementation, rewrite body to `return-void`.
  - `frida_gadget.rs` — Inject `libfrida-gadget.so` into `lib/<abi>/`, add `android:extractNativeLibs="true"` to `<application>`, ensure `System.loadLibrary("frida-gadget")` runs early (inject into `Application.onCreate`).
  - `debuggable.rs` — Toggle `android:debuggable="true"` on `<application>`.
  - `webview_debug.rs` — Find every `setWebContentsDebuggingEnabled(false)` smali call and patch the const to `0x1`.
  - `nsc_bypass.rs` — Generate a permissive `network_security_config.xml` allowing user CAs, wire it into `<application android:networkSecurityConfig="@xml/nsc">`.
  - `proxy_ca.rs` — Drop a user CA cert under `res/raw/` and wire into NSC.

**How decode/build plug together (round-trip invariant):**

1. `decode` writes `apktool.yml` capturing every detail rebuild needs: original `minSdkVersion`, `targetSdkVersion`, `versionCode`, `versionName`, which entries were STORED vs DEFLATED, the list of "unknown files" (anything not recognized — kept byte-identical and replayed during build), and any sparse-resource markers.
2. Smali files carry `.class`, `.super`, `.implements`, `.field`, `.method` directives that fully round-trip to DEX (the bytecode + try/catch + debug info + annotations).
3. AXML decode preserves resource ref encoding so re-encoding produces the same binary (modulo string pool ordering — which is canonicalized).
4. ARSC decode → res/ tree → ARSC re-encode is the lossy step; we rely on AAPT2 to canonicalize. For round-trip safety on resources we can also use `-c copy-original` (replay original `resources.arsc` and `AndroidManifest.xml`) bypassing AAPT2 — useful for surgical patches.

## Granular feature catalog

**CLI:** `lancer-apktool <subcommand> [args]`.

- `decode` / `d`
  - `-o <dir>` — output directory (default: APK basename).
  - `-f` — force overwrite if output dir exists.
  - `-r` / `--no-res` — skip resource decoding (just extract `resources.arsc` verbatim).
  - `-s` / `--no-src` — skip smali disassembly (just extract `classes*.dex`).
  - `--frame-path <dir>` — override framework cache location.
  - `--frame-tag <tag>` — pick a tagged framework variant (e.g., `samsung`).
  - `--keep-broken-res` (upstream v3.0+ replaces the old `-k` short flag) — keep partially-decoded resources when ARSC has version-mismatched chunks.
  - `-a` / `--all-src` (upstream v3.0+ replaces `--only-main-classes`; default in v3.x is main classes only, `--all-src` includes secondary DEX) — include every `classes*.dex` in the decode.
  - `--match-original` (upstream v3.0+ — short `-m` retired) — try to preserve original byte layout (disables some normalization).
  - `--no-debug-info` (upstream v3.0+ — short `-b` retired) — strip smali debug info / line numbers.
  - `--res-resolve-mode <default|greedy|lazy>` (upstream v3.0+ renames the historical `remove`/`dummy`/`keep`) — `default` emits `APKTOOL_DUMMY_*` only for unresolvable refs; `greedy` for all missing; `lazy` ignores missing.
  - `--ignore-raw-values` (upstream v3.0+) — skip raw-value decoding to avoid AAPT2-incompatible attribute payloads.
  - `--api <level>` — accepted for backward compatibility only; in upstream v3.0+ this flag is **removed** because API level is auto-detected from `targetSdkVersion`/`compileSdkVersion`. `lancer-apktool` keeps it as an override for hostile / malformed manifests where auto-detect fails.
  - `--jobs <n>` (upstream v2.10+) — parallel decode (one thread per DEX baksmali job + resource walker).
  - Short-flag chaining (upstream v3.0+): `-vfo` parses as `-v -f -o`.

- `build` / `b`
  - `-o <apk>` — output APK path.
  - `-f` — force rebuild even if outputs look up to date.
  - `--frame-path <dir>` — framework cache.
  - `--copy-original` (upstream v3.0+ — short `-c` retired) — replay original `AndroidManifest.xml` + `META-INF` from the project (skip AAPT2 re-compile of resources).
  - `--aapt <aapt-binary>` (upstream v3.0+ — short `-a` retired) — explicit AAPT2 binary path.
  - `--debuggable` (upstream v3.0+ — short `-d` retired) — force `android:debuggable="true"` during build.
  - `--net-sec-conf` (upstream v3.0+ — short `-n` retired) — auto-inject a permissive `network_security_config.xml` or extend the existing one with user/system CAs (added v2.11.0).
  - `--no-apk` (upstream v3.0+ — short `-na` retired) — produce the build artifacts without packaging the final APK (useful for chained tooling).
  - `--no-crunch` (upstream v3.0+ — short `-nc` retired) — skip PNG optimization.
  - `--no-compress <pattern>` — keep matching entries STORED.
  - `--use-aapt2` — accepted for backward compatibility. **`--use-aapt1` is removed in upstream v3.0+** (aapt1 fully dropped). `lancer-apktool` matches: aapt2 only.
  - `--jobs <n>` (upstream v2.10+) — parallel build (one thread per smali assemble job; resources stay single-threaded since AAPT2 is the bottleneck).
  - `--reproducible` (upstream v2.10+) — strip timestamps/order-dependent fields so identical inputs produce byte-identical APKs.
  - `--dynamic-lib-id <pkg>=<id>` (upstream v2.12.0) — assign explicit package IDs to shared/dynamic libraries on rebuild.

- `install-framework` / `if`
  - `<apk>` — framework APK to ingest into the cache.
  - `-p <dir>` — target framework directory.
  - `-t <tag>` — tag this framework variant.

- `clean-frameworks` / `cf` (upstream v3.0+ — renamed from `empty-framework-dir` / `efd`) — wipe the framework cache directory (drop in case of corruption).
  - `-a` / `--all` — clear every tag.
  - `-p` / `--frame-path <dir>` — target a specific cache dir.
  - `-t` / `--frame-tag <tag>` — clear only a specific tag.

- `empty-framework-dir` / `efd` — accepted as a deprecated alias of `clean-frameworks` for muscle-memory compatibility.

- `publicize-resources <apk>` — flip every resource's `<public>` flag (rare; for theming work and OEM mods).

- `sign <apk>` (Lancer addition)
  - `--keystore <file>` — JKS / PKCS12 keystore.
  - `--alias <name>` — key alias.
  - `--storepass <pw>`, `--keypass <pw>` — passwords.
  - `--scheme v1,v2,v3,v4` — which signature schemes to apply.
  - `--debug` — use bundled debug keystore (default).

- `diff <a.apk> <b.apk>` (Lancer addition)
  - `--smali` — smali AST diff only.
  - `--resources` — resource diff only.
  - `--manifest` — manifest semantic diff only.
  - `--json` — machine-readable output.

- `patch <apk> --preset <name>` (Lancer addition)
  - `--preset pin-bypass | frida-gadget | debuggable | webview-debug | nsc-bypass | proxy-ca`

**Smali assembler features (`smali` crate):**

- Full DEX bytecode: ~250 opcodes (`nop`, `move`, `move-wide`, `move-object`, `return-void`, `return`, `return-wide`, `return-object`, `const`, `const-4/16/high16/wide`, `const-string`, `const-string/jumbo`, `const-class`, `monitor-enter`/`exit`, `check-cast`, `instance-of`, `array-length`, `new-instance`, `new-array`, `filled-new-array`, `fill-array-data`, `throw`, `goto`, `packed-switch`, `sparse-switch`, `cmpl-float`, `cmpg-float`, `cmpl-double`, `cmpg-double`, `cmp-long`, `if-eq/ne/lt/ge/gt/le`, `if-eqz/nez/ltz/gez/gtz/lez`, `aget`, `aput`, `iget`, `iput`, `sget`, `sput` (all type variants), `invoke-virtual/super/direct/static/interface`, `invoke-*-range`, `invoke-polymorphic`, `invoke-custom`, all arithmetic ops (`add`, `sub`, `mul`, `div`, `rem`, `and`, `or`, `xor`, `shl`, `shr`, `ushr`) for int/long/float/double in `2addr` and `lit8`/`lit16` variants).
- Directives: `.class`, `.super`, `.implements`, `.source`, `.field`, `.method`, `.end method`, `.registers`, `.locals`, `.param`, `.parameter`, `.prologue`, `.line`, `.local`, `.end local`, `.restart local`, `.catch`, `.catchall`, `.array-data`, `.packed-switch`, `.sparse-switch`, `.annotation`, `.subannotation`, `.enum`.
- Annotations: runtime/build/system visibility, nested annotations, array values, enum references.
- Access flags: full set (`public`, `private`, `protected`, `static`, `final`, `synchronized`, `volatile`, `bridge`, `transient`, `varargs`, `native`, `interface`, `abstract`, `strictfp`, `synthetic`, `annotation`, `enum`, `constructor`, `declared-synchronized`).
- Try/catch ranges, debug info (line numbers, local var lifetimes).
- Multi-DEX: scan for `classes2.dex`, `classes3.dex`, ... write into `smali_classes2/`, etc.
- Multi-DEX containers (upstream v3.0+): a single APK entry can package multiple DEX streams concatenated — walk transparently and split into separate smali trees.
- API-level dialect awareness: invoke-polymorphic (API 26+), invoke-custom (API 26+), const-method-handle/type (API 28+), API 35 (Vanilla Ice Cream), API 36/36.1 (Baklava).
- Track upstream `smali`/`baksmali` 3.0.9+ for new opcode tweaks and feature-flag-guarded instructions added in SDK 35.

**AXML encoder/decoder features (`axml` crate):**

- Binary XML chunk header parsing (`RES_XML_TYPE` = 0x0003).
- Sub-chunks: `RES_STRING_POOL_TYPE` (0x0001), `RES_XML_RESOURCE_MAP_TYPE` (0x0180), `RES_XML_START_NAMESPACE_TYPE` (0x0100), `RES_XML_END_NAMESPACE_TYPE` (0x0101), `RES_XML_START_ELEMENT_TYPE` (0x0102), `RES_XML_END_ELEMENT_TYPE` (0x0103), `RES_XML_CDATA_TYPE` (0x0104).
- String pool: UTF-8 vs UTF-16 mode, sorted vs unsorted flag, style spans.
- Resource map: ID → attribute name binding so encoder knows which attribute IDs are well-known Android attrs.
- Namespace stack: track prefix↔URI bindings across element start/end pairs.
- Attribute typed values: `TYPE_NULL`, `TYPE_REFERENCE`, `TYPE_ATTRIBUTE`, `TYPE_STRING`, `TYPE_FLOAT`, `TYPE_DIMENSION`, `TYPE_FRACTION`, `TYPE_DYNAMIC_REFERENCE`, `TYPE_INT_DEC`, `TYPE_INT_HEX`, `TYPE_INT_BOOLEAN`, `TYPE_INT_COLOR_*`.
- Text XML emission: resolve `@ref` strings, preserve `android:` prefix.

**ARSC encoder/decoder features (`arsc` crate):**

- Top-level `ResTable_header`.
- Per-package `ResTable_package` chunks.
- Type strings + key strings string pools.
- `ResTable_typeSpec` (per-resource config flags).
- `ResTable_type` (per-config table): contains entries flagged sparse / dense / offset16.
- Config qualifier matching: MCC, MNC, locale (BCP-47 in v23+), screen layout (`small`, `normal`, `large`, `xlarge`), orientation (`port`, `land`), UI mode (`car`, `desk`, `television`, `appliance`, `watch`, `vrheadset`), night mode, density (`ldpi`/`mdpi`/`hdpi`/`xhdpi`/`xxhdpi`/`xxxhdpi`/`tvdpi`/`anydpi`/`nodpi`), touch screen, keyboard, navigation, screen size, screen aspect, round screen, wide color gamut, HDR, API level.
- Complex entries (style/array/plurals): `ResTable_map_entry` with parent + child key/value list.
- Sparse encoding (Android 8+): map of (index → offset) instead of dense offset array.
- Type 8 (RES_TABLE_LIBRARY_TYPE) for shared library resource packages.
- Type 9 (RES_TABLE_OVERLAYABLE_TYPE) for OEM RRO overlays.

**Framework cache:**

- Default `1` = stock Android framework-res.apk.
- OEM/version-specific frameworks installed via `if`.
- Tag system so `framework-1-samsung.apk` coexists with `framework-1.apk`.

**Resource resolution modes (upstream v3.0+ rename):**

- `default` (was `remove`) — only resources that genuinely cannot be resolved get an `APKTOOL_DUMMY_*` placeholder; everything else is emitted normally.
- `greedy` (was `dummy`) — every missing-from-arsc entry gets a placeholder, even when it could be resolved with more work.
- `lazy` (was `keep`) — drop the placeholder dance entirely; missing refs leave holes for the rebuilder to deal with.
- Placeholder name is now uniformly `APKTOOL_DUMMY_<n>` (the historical `APKTOOL_MISSING_*` form is retired).

**`apktool.yml` schema (upstream v3.0+ rename):**

- `resourcesInfo:` (was `packageInfo:`) — block holds package ID + rename map.
  - `packageId:` (was `forcedPackageId:`) — the integer ID under which resources were originally allocated.
- `compactEntries:` (added v2.11.0) — true when the source ARSC used the compact entry format, so the writer must restore it on rebuild.
- `sharedLibrary:` and `sparseResources:` — preserved as before.
- `usesFramework:` — list of framework IDs + tags the original APK depended on.

**Output normalization (upstream v3.0+ modernization — `lancer-apktool` matches by default, controllable via `--legacy-output`):**

- Hex values written in compact form (`0x20`, not `0x00000020`).
- Color attributes preserved in their original APK formatting (`#ff8800`, `#f80`, `#ffff8800` as they were).
- Resource type aliasing: emit `<dimen name="...">` instead of `<item type="dimen" name="...">`.
- `dp` replaces the legacy `dip` suffix on dimension values.
- `match_parent` replaces the legacy `fill_parent` enum on layout attributes.
- Flag attributes filter redundant duplicate flag bits (v2.12.0).
- CDATA blocks preserved through round-trip (v2.12.0 fix).

**Split APK / shared library handling (upstream v3.0.2+):**

- A base APK or a config split can be loaded **as a library** during decode of another split, so cross-split resource references resolve cleanly without a full AAB merge step.
- `lancer-apktool decode --as-library <base.apk> <config-split.apk>` mirrors upstream PR #4128.
- Staged aliases (preview-API resource IDs that get remapped to final IDs at bake time) are recognized and logged as `Skipping staged alias` — placeholder pointer chases the final ID.
- Multi-DEX containers inside a single split are walked transparently.

## Engineering hard parts

- **Smali assembler round-trip safety.** The mechanical bit (250 opcodes) is the easy part. The hard part is keeping a decode → encode pass byte-identical (or semantically equivalent): register allocation must reuse what the original DEX had, instruction selection between `const` and `const/4`/`const/16`/`const/high16` must pick the smallest form the original picked, string-pool ordering must be deterministic, and constant pool deduping must match. This is what makes patched APKs runnable instead of bricking on `VerifyError`.

- **AXML resource ref resolution.** A binary AXML attribute pointing at `0x7f0a0001` means "string resource #1 in app's type 10". Resolving that against the app's own `resources.arsc` is straightforward; resolving framework refs (`0x01010003` = `android:label`) requires the framework cache and a copy of `android-1.apk`'s ARSC tables. Get this wrong and the output XML is unreadable.

- **ARSC config qualifier matching.** Android's resource resolver picks the "best" qualifier set for a given device config. Encoding qualifiers back into ARSC chunks requires getting the bit layout exactly right (`ResTable_config` is a packed C struct), including post-v23 BCP-47 locale encoding (variable length, snuck in via the unused locale bytes).

- **AAPT2 dependency.** Google's `aapt2` is the canonical resource compiler. Porting it to pure Rust is a major project — it does PNG crunching, 9-patch processing, AAPT-style binary XML compilation, ARSC linking. v1 will shell out to a bundled `aapt2` binary; longer-term we ship a Rust port covering the subset APKTool exercises.

- **Android signing scheme v1/v2/v3/v4.** v1 is JAR signing (MANIFEST.MF + signature file + RSA block in `META-INF`). v2 inserts an "APK Signing Block" into the zip between central directory and last entry, containing a v2 signature over a digest of zip contents. v3 adds key rotation. v4 (Android 11+) lives in a sidecar `.apk.idsig` for incremental install / Play AB streaming. Implementing all four correctly requires careful zip-rewriting that doesn't shift offsets the central directory points to.

- **Multi-DEX handling.** Apps over 64K methods split into `classes.dex`, `classes2.dex`, ... Decode → `smali/`, `smali_classes2/`, ... Rebuild must merge back, respecting the original DEX-per-class mapping (so the same class doesn't migrate DEX files, breaking dex-method-ref limits).

- **AAB (App Bundle) split APK handling.** Modern Play Store distribution is `.aab` → multiple split APKs (base + per-language + per-density + per-ABI). For pentest you usually need to merge splits back into a single fat APK before patching. Implementing this means understanding the `BundleConfig.pb` protobuf and the split APK layout.

- **Signing v4 requires Merkle tree.** The `.apk.idsig` sidecar contains a SHA-256 Merkle tree over the APK bytes. Re-signing v4 means recomputing it; we lean on `merkle-tree` crate plus careful chunking matching Android's `fs-verity` page size.

- **PNG crunching parity.** Android packs 9-patch PNGs (`.9.png`) into a custom format consumed by the runtime. AAPT2 does this; a Rust port needs to read 9-patch chunks (`npTc`, `npLb`), output them in AAPT2's expected layout, ideally pixel-equivalent.

- **Compression bookkeeping.** Decode-then-build can quietly switch a STORED entry to DEFLATED, breaking apps that mmap `resources.arsc` directly. We track per-entry compression in `apktool.yml`.

- **Smali AST diff (Lancer extension).** Text diff of smali is noisy because register numbers and label IDs change across rebuilds. The diff engine must parse smali into a normalized AST, alpha-rename registers, then diff the AST. Useful for supply-chain review.

- **Parallel build correctness.** Upstream v2.10.0 introduced one-thread-per-DEX baksmali/smali parallelism (`--jobs N`, default = core count). The hard part is not the threading — it is keeping output byte-deterministic when threads finish in non-deterministic order: string-pool allocation, ARSC entry ordering, and zip entry order all have to be canonicalized post-collection so `--reproducible` produces identical bytes across runs. We get this for free from a join-then-sort step before emit.

- **Reproducible builds.** Strip every timestamp source: zip entry mtime → epoch 0, manifest build-tool stamp → blank, AAPT2 `--non-final-ids` to lock IDs, sorted iteration of resource configs. Match upstream v2.10+ behavior so a `lancer-apktool build --reproducible` of an upstream-decoded project produces a byte-identical APK to upstream's own build.

- **ApkEditor "protect mode" defense (upstream v2.12.0).** ApkEditor and similar tools insert malformed chunk headers, zero-length string pool entries, or sentinel package IDs designed to make decoders crash or refuse to decode. The decoder needs hardened bounds-checking, sentinel skipping, and explicit user-facing warnings ("this APK ships protect-mode markers from ApkEditor — decoded anyway") instead of bailing out. Matches upstream's posture: decode-and-warn beats fail-closed.

- **Staged aliases (upstream v3.0+).** Preview-API SDK builds (e.g., Baklava preview) ship resource IDs like `0x01ff0001` that will be remapped to a stable ID at platform bake time. The ARSC walker has to detect the staged-alias chunk, log the alias source→target mapping, and decode the resource using the target ID so the output XML is correct. Without this, preview-SDK APKs decode with garbled framework refs.

- **`network_security_config.xml` extension (upstream v2.11.0).** When the target app already ships a custom NSC, the `--net-sec-conf` build flag should **merge** new `<certificates src="user"/>` and `<certificates src="system"/>` entries into the existing config instead of clobbering it. This is the bug-bounty workflow most users actually want — bypass pinning while preserving the app's own NSC structure so static-analysis review still recognizes the file.

- **Feature-flag-guarded resources (upstream v2.11.0).** SDK 35+ ships `<feature-flag name="..." />` gating on manifest elements and resources. The encoder must round-trip these flags through ARSC/AXML without dropping them; the decoder must surface them in the output XML so a tester can flip a flag during a patch.

- **API auto-detection (upstream v3.0+).** Upstream removed `--api-level`; the decoder reads `targetSdkVersion` / `compileSdkVersion` / `platformBuildVersionCode` from the binary manifest and passes that into baksmali to pick the right opcode dialect. `lancer-apktool` keeps `--api` as an override for hostile manifests (some malware ships `targetSdkVersion=0`) but follows the auto-detect path by default.

- **Binary XML parser modernization (upstream v3.0+).** The v3.x rewrite replaced the historical AXML parser with `BinaryXmlResourceParser` + `ResXmlSerializer` (proper SAX-like streaming, no full DOM in memory). We mirror this — our `axml` crate already streams; the v3.x design validates that approach. Bonus: streaming makes memory bounded for huge merged-split manifests.

## Rust library mapping

| Need | Crate | Notes |
|---|---|---|
| APK / zip read+write | `zip` (0.6+) | Standard. Need `read_with_offset` for v2-sig support. |
| Per-entry compression control | `zip::CompressionMethod` | STORED vs DEFLATED toggling. |
| DEFLATE codec | `flate2` | miniz-oxide backend, pure Rust. |
| Binary AXML / ARSC parsing | custom `axml` + `arsc` crates (this workspace) | No existing Rust crate covers both. |
| Byte-order helpers | `byteorder` | Little-endian throughout Android binary formats. |
| Zero-copy parsing | `zerocopy` 0.7 | Optional; speeds up ARSC chunk scanning. |
| Smali parsing | `pest` or `chumsky` parser combinators | Smali grammar is regular-enough. |
| Smali → DEX bytecode | hand-rolled emitter (this workspace) | DEX format is documented; ~30 chunk types. |
| DEX → Smali disassembler | hand-rolled (this workspace) | Direction inverse of above. |
| Resource compilation | `aapt2-bridge` (shells to bundled `aapt2` binary) | Pure-Rust port deferred. |
| PNG read/write | `png` 0.17 | For 9-patch handling and PNG crunch. |
| JKS / PKCS12 keystore read | `jks` (community) + `p12` | Default debug keystore bundled. |
| PKCS#7 signing (v1) | `pkcs7` + `cms` | For META-INF/CERT.RSA. |
| RSA / ECDSA / Ed25519 | `rsa`, `p256`, `ed25519-dalek` | v2/v3 signing keys. |
| X.509 certificates | `x509-cert` + `der` | Cert chain encoding. |
| SHA-256 / SHA-1 digests | `sha2`, `sha1` | For MANIFEST.MF + v2 digest blocks. |
| Merkle tree (v4 signing) | `rs_merkle` or hand-roll | Page-size chunking matches fs-verity. |
| Zipalign | hand-rolled | Walk entries, pad uncompressed data to 4-byte (and 4K for `lib/*.so`). |
| CLI | `clap` 4.x | Standard. |
| Async runtime (optional) | `tokio` | For ADB streaming + install. |
| ADB streaming install | `adb_client` (community) | For `--install` flag. |
| Logging | `tracing` 0.1 | Standard. |
| Errors | `eyre` 0.6 | Standard. |
| YAML (`apktool.yml`) | `serde_yaml` | Standard. |
| Protobuf (`BundleConfig.pb`) | `prost` 0.12 | For AAB handling. |

(For the AAPT2 port path: `tree-sitter-xml` for resource XML parsing, custom PNG crunching loop, `zerocopy` for the ARSC writer.)

## Bug bounty workflows

- **SSL pin-bypass repack.** App pins a cert via `TrustManager.checkServerTrusted`. Decode APK → `lancer-apktool patch --preset pin-bypass` → rebuild → resign with debug key → install → traffic now visible to your proxy. Standard kickoff for any mobile bounty.

- **Frida-gadget injection.** Target app blocks rooted devices / Frida server attachment. Instead, statically inject `libfrida-gadget.so` + load instruction. `lancer-apktool patch --preset frida-gadget` adds the `.so` to `lib/<abi>/`, patches `Application.onCreate` smali to `System.loadLibrary("frida-gadget")`. App now self-instruments on launch, no root required.

- **`debuggable=true` patch.** Flip `<application android:debuggable>` to `true` → JDWP attach with `adb jdwp` → live class loading + bytecode inspection. One-liner: `lancer-apktool patch --preset debuggable`.

- **WebView debug enable.** Apps that disable `setWebContentsDebuggingEnabled` block Chrome DevTools attach. Patch every const-bool `0x0` passed to that call to `0x1` → `chrome://inspect` lists the WebView. Useful for hybrid apps.

- **network_security_config bypass.** Android 7+ apps that pin via NSC (the modern way) ignore user-installed CAs by default. `lancer-apktool patch --preset nsc-bypass` generates a permissive `network_security_config.xml`, wires it into `<application android:networkSecurityConfig>`, repacks.

- **Proxy CA add.** Drop your Burp/mitmproxy CA into `res/raw/` and reference it from a custom NSC. After repack, app trusts that CA system-wide (within the app's sandbox).

- **AAB → fat APK.** Target ships as Play Store AAB. Pull splits via Bundletool, merge via `lancer-apktool aab merge`, then standard decode/patch/rebuild flow.

- **Patch-diff a security update.** App version 5.1.0 patches a CVE; version 5.0.9 doesn't. `lancer-apktool diff v509.apk v510.apk --smali` highlights every smali method that changed → focus reverse engineering on those.

- **Supply-chain verification.** Compare APK from Play Store vs APK from a third-party mirror. `lancer-apktool diff` flags anything beyond signing differences → quickly spot tampered builds.

- **Resource theft / cloning detection.** When defending: diff a suspect competitor APK against yours. Manifest + resource diff exposes copied IDs, strings, drawables.

## 2024–2026 state

- **Apktool 2.9.0–2.9.3 (Oct 2023 – Jan 2024):** stabilized AAPT2-by-default pipeline; sparse encoding fully supported in ARSC writer; improved support for Android 14 (API 34) resources including BCP-47 locale corner cases; AGP 8.2 namespace migration handled. 2.9.1 added `android:isAccessibilityTool` and de-duped style attribute names. 2.9.2 and 2.9.3 were security releases (CVE-class arbitrary-file-write on Windows: GHSA-2hqv-2xv4-5h5w, GHSA-vgwr-4w3p-xmjv).

- **Apktool 2.10.0 (Sept 2024):** **parallel building** via `--jobs N` (one thread per baksmali/smali task, resources kept single-threaded), **reproducible builds**, auto-detection of the latest apktool.jar, API 34 framework update, foreground-service-type manifest attribute, fixed `stamp-cert-sha256`, smali/baksmali bumped to 3.0.8, Gradle 8.7.

- **Apktool 2.11.0 (Jan 2025) + 2.11.1 (Mar 2025):** **API 35 (Vanilla Ice Cream)** preview + **Android Baklava Developer Preview 1** support, feature-flags support for SDK 35, **compact entries** building, **`--net-sec-conf` extension** that merges user/system CA entries into existing network security configs instead of clobbering them, ApkDecoder/ApkBuilder refactor, smali/baksmali 3.0.9, WhatsApp un-ordered-resource fix, table-spec out-of-bounds hardening.

- **Apktool 2.12.0 (Jul 2025) + 2.12.1 (Sept 2025):** **API 36 (Baklava)** support, **`--dynamic-lib-id` CLI parameter** for shared-library package ID assignment, **ApkEditor "protect mode" defense**, redundant flag-attribute bits filtered, AAPT2 binaries upgraded to Android 16 sources, CDATA round-trip fix (vbfox), test suite fully migrated from aapt1 to aapt2 (aapt1 deprecation begins).

- **Apktool 3.0.0 (Jan 17, 2026) — major rewrite ("Apktool Remastered"):** **aapt1 fully removed** (aapt2 only), **32-bit platforms dropped**, **`-api/--api-level` removed in favor of auto-detection**, **short-flag chaining** (`-vfo` = `-v -f -o`), **`--only-main-classes` replaced by `-a/--all-src` with main-classes-only as the new default**, advanced options moved to long-form only. Renamed: `efd`/`empty-framework-dir` → `cf`/`clean-frameworks` with `-a/-p/-t` flags. Decode flag renames: `-b`→`--no-debug-info`, `-k`→`--keep-broken-res`, `-m`→`--match-original`, `-resm`→`--res-resolve-mode`. Build flag renames: `-a`→`--aapt`, `-c`→`--copy-original`, `-d`→`--debuggable`, `-n`→`--net-sec-conf`, `-na`→`--no-apk`, `-nc`→`--no-crunch`. New `--ignore-raw-values` decode flag. Resource modes renamed `remove/dummy/keep` → `default/greedy/lazy`; `APKTOOL_MISSING_*` retired in favor of `APKTOOL_DUMMY_*`. apktool.yml renames: `packageInfo` → `resourcesInfo`, `forcedPackageId` → `packageId`. Output modernization: `dp` (not `dip`), `match_parent` (not `fill_parent`), hex compact form `0x20`, color values preserved as written, resource types aliased (`<dimen>` not `<item type="dimen">`). **API 36.1 (Baklava QPR)** support. **Staged aliases** for preview-SDK resource IDs. **Multi-DEX containers**. New **`BinaryXmlResourceParser` + `ResXmlSerializer`** streaming implementations. Gradle 8.14.4, R8 9.0.32.

- **Apktool 3.0.1 (Feb 2026):** re-tag of 3.0.0 because the original 3.0.0 artifact was missing the published `.jar`.

- **Apktool 3.0.2 (Apr 19, 2026):** **~19% disassembly perf improvement** via buffered I/O + faster `ResPackage` entry lookup. **Split-APK as library** (PR #4128) — base APK or a config split can be loaded as a library while decoding another split, fixing cross-split refs without a full AAB merge. ApkDecoder allows ApkInfo retrieval post-decode. aapt2 binaries refreshed.

- **2026 dev tip:** Android 16 preview APIs (Baklava QPR) leaking into resource qualifiers; `.aab` toolchain is dominant — pure single-APK distribution is rare on Play but ubiquitous off-Play. Smali API levels at 35/36/36.1. `apksigner` v4 (`.apk.idsig`) signing now expected for any APK pushed via `adb install --incremental`. Upstream Apktool **still does not sign APKs natively** — `apksigner.jar` from Build Tools remains the recommended signer. This is a clean differentiation lane for `lancer-apktool`: ship native v1/v2/v3/v4 signing as part of `build`, no external `apksigner` required.

- **Ecosystem:** `jadx` (read-only) remains the partner tool. `frida-server` 16.x updated for Android 15/16. `objection` automation around APKTool + frida still standard. New entrants: `apkleaks` (secret scanning over decoded smali), `MobSF` (UI on top of APKTool + drozer + others), `apkpatcher` (Python wrapper that scripts the pin-bypass flow). **ApkEditor** is a hostile actor in this space — its "protect mode" tries to break decoders; upstream v2.12.0 added defenses that we must mirror. **REAndroid / ARSCLib** is the emerging non-Apktool decoder (used by H-K-S APK-JAR TOOL); we cross-test against it.

- **Legal landscape:** Apache-2.0; redistribution OK. We can ship `lancer-apktool` as a single static Rust binary, no JVM required — that alone is a huge UX improvement vs. upstream's fat-JAR-plus-wrapper-script delivery.

- **Note added 2026-05-30 (deep-research sync):** Re-verified upstream timeline against `apktool.org/blog` and GitHub releases. The earlier draft of this section listed Apktool 2.10.x as 2025 — corrected: 2.10.0 = Sept 2024, the 2.11.x line = early/mid 2025, 2.12.x = mid/late 2025, and the 3.0.x line = Jan-Apr 2026. Parity targets bumped from "2.10.x" to **upstream v3.0.2 (April 19, 2026)** as the floor. Specifically called out as new parity work since the original plan: short-flag chaining, every renamed decode/build flag in v3.0, the `clean-frameworks`/`cf` rename, the resource-mode rename (`default`/`greedy`/`lazy`), `APKTOOL_DUMMY_*` (vs. retired `APKTOOL_MISSING_*`), `resourcesInfo`/`packageId` apktool.yml schema, `dp`/`match_parent`/compact-hex output normalization, `--ignore-raw-values`, `--jobs N` parallel build, `--reproducible`, `--dynamic-lib-id`, `--net-sec-conf` NSC merging, API auto-detection, compact entries, staged aliases, multi-DEX containers, ApkEditor protect-mode defense, split-APK-as-library decode, BinaryXmlResourceParser/ResXmlSerializer streaming model, smali/baksmali 3.0.9 features, API 35 / 36 / 36.1 (Baklava + Baklava QPR) framework support. APK signing v4 (`.apk.idsig`) confirmed as native Lancer-side feature since upstream still defers to external `apksigner`. AAB merge stays a Lancer extension — upstream still has no first-class `.aab` ingestion, only the new split-APK-as-library trick that partially substitutes.

## AI augmentation ideas

- **Explain a smali block.** Highlight a smali method in the Lancer UI, click "Explain" → LLM gets the smali + surrounding class context, returns a plain-English description ("This method decrypts an AES-128-CBC blob using a hard-coded key from `static final byte[] KEY`."). Lets a tester triage 500-method classes in minutes.

- **Auto-generate "make this APK debuggable" patch.** User uploads an APK, asks "make this debuggable and bypass pinning." LLM emits a sequence of `lancer-apktool patch --preset` calls and any custom smali edits needed beyond the presets.

- **Auto-rewrite `network_security_config.xml`.** Given a target's existing NSC, generate a derived permissive config that still passes static analysis ("looks reasonable to a human reviewer") but trusts the tester's CA.

- **Crypto routine recognition in smali.** Embed every method, vector-search for known crypto patterns (AES key expansion, RC4 init, OAEP padding). Auto-rename methods accordingly.

- **String decryption assistance.** Detect obfuscated string-decryption methods (small static methods returning `String`, taking `int` index args, doing XOR loops), run them symbolically and pre-decrypt every call site in the smali listing.

- **Pin-bypass smali synthesis.** For pinning implementations the canned preset doesn't catch (Conscrypt-based custom validators, OkHttp `CertificatePinner` subclasses), feed the relevant class to the LLM and ask for a tailored smali patch.

- **Manifest hardening review.** "Review this AndroidManifest for misconfigurations" → LLM flags exported components, missing permission gates, `android:allowBackup="true"` on sensitive apps.

- **Diff explanation.** After `lancer-apktool diff`, LLM summarizes "the v5.1.0 update added an HMAC check to `LoginRequest.sign`, changed three string constants, and removed an unused `WRITE_EXTERNAL_STORAGE` permission."

- **Auto-generate a Frida script.** Given a class and method picked in the UI, LLM emits a Frida hook script (`Java.use("com.target.X").method.implementation = ...`) using the decoded smali signature.

- **Resource-string semantic search.** "Find every string in this APK that looks like an API endpoint or hard-coded key" — LLM scans the decoded `strings.xml` + `assets/`.

## Implementation phases (10-week clone)

**Week 1 — Workspace + zip + AXML decode.** Cargo workspace (`lancer-apktool`, `apktool-core`, `axml`, `arsc`, `smali`, `apksig`, `aapt2-bridge`). Implement `axml` decode: round-trip a real `AndroidManifest.xml` to text and back. Test against 20 sample APKs of varying API levels.

**Week 2 — AXML encode + ARSC decode.** Finish AXML round-trip. Start `arsc` decode: walk all chunk types (`RES_TABLE_*`), emit `res/values/strings.xml`, `res/values/colors.xml`, `res/values-<qualifier>/*.xml`. Resolve resource refs against framework cache.

**Week 3 — Framework cache + decode pipeline.** Implement `if`/`install-framework` + `empty-framework-dir`. Wire decode pipeline end-to-end (manifest + resources + asset extract + lib extract). Ship `apktool.yml` writer capturing compression and unknown-file state.

**Week 4 — Smali disassembler.** DEX format reader (header, string IDs, type IDs, proto IDs, field IDs, method IDs, class defs, data section). Emit smali for every class. Round-trip test against a corpus: disassemble → byte-compare against `baksmali` output.

**Week 5 — Smali assembler.** Reverse: smali parser (pest grammar) → DEX writer. Most-likely-correct register allocation matching original. Round-trip test: disassemble → reassemble → DEX byte-compare.

**Week 6 — ARSC encode + AAPT2 bridge.** Either ship a bundled `aapt2` binary and shell to it, or write the resource compile path for simple cases (`strings.xml` + `colors.xml` → ARSC). Wire `build` subcommand.

**Week 7 — Build pipeline + zip + zipalign.** Reverse pipeline: smali → DEX, text XML → AXML, source res → ARSC (via aapt2-bridge), zip per `apktool.yml` (honoring per-entry compression), zipalign.

**Week 8 — APK signing v1/v2/v3.** Bundle debug keystore. Implement v1 (PKCS#7), v2 (APK Signing Block), v3 (rotation). Test against `apksigner verify`. Signing v4 deferred to phase 2 (needs Merkle tree + `.apk.idsig`).

**Week 9 — Pentest presets + diff mode.** Wire `patch --preset pin-bypass | frida-gadget | debuggable | webview-debug | nsc-bypass | proxy-ca`. Implement `diff` subcommand (smali AST diff + manifest semantic diff + resource diff).

**Week 10 — AAB support + AI hooks + polish.** AAB merger (`aab merge` → fat APK). Wire MCP endpoints exposing decode/build/patch as tools for Claude/Cursor. Lancer Mobile Assistant tab integration: pick APK → decode → edit smali in built-in editor → one-click rebuild + sign + ADB install.

---

## GUI (Lancer-standard pattern)

`lancer-apktool` ships as a **native GUI application** by default. The CLI surface listed in `## Granular feature catalog` is still available behind a `--cli` flag (or via the `lancer-apktool-cli` companion binary), but the **plain `lancer-apktool` invocation opens a native desktop window**, matching every other tool in the Lancer ecosystem. No terminal-only mode.

### Framework

Pure-Rust GUI stack — no JavaScript, no Node, single static binary:

| Crate | Role |
|-------|------|
| `eframe` | Cross-platform window + event loop wrapper around `egui` |
| `egui` | Immediate-mode UI widgets (panels, tables, forms, menus, drag-and-drop) |
| `egui_extras` | Sortable tables, image loaders, syntax highlighting, date pickers |
| `egui_plot` | Charts, graphs, timelines (histograms, activity meters, progress) |
| `rfd` | Native file/folder pickers (open file, save dialog) |
| `directories` | Per-OS config dir (`~/.config/lancer-apktool/`, `%APPDATA%\lancer-apktool\`, etc.) |
| `image` | Screenshot / preview rendering inside egui |
| `arboard` | Clipboard read/write for "copy result" actions |
| `muda` | Native menu bar (Linux / macOS / Windows) |
| `egui-notify` | Toast notifications |

Optional dark/light theme persisted to `lancer-apktool-config.json` via `serde_json`.

### Window layout

Three-pane layout matching the Lancer mothership GUI:

```
┌─────────────────────────────────────────────────────────────────────┐
│  File   Tools   Settings   Help                              [≡]    │
├──────────┬────────────────────────────────────────┬─────────────────┤
│          │                                        │                 │
│  Inputs  │           Main view                    │  Detail pane    │
│  / jobs  │  (table of results / source tree /     │  (full record / │
│  / scope │   findings list / packet list / etc.)  │   bytes / src / │
│          │                                        │   remediation)  │
│          │                                        │                 │
├──────────┴────────────────────────────────────────┴─────────────────┤
│  Status: idle           Items: 0          [Open]  [Run]  [Export]   │
└─────────────────────────────────────────────────────────────────────┘
```

- **Left sidebar (200-300px)** — inputs / queued jobs / scope / target list. Drag-and-drop accepted.
- **Center pane (flexible)** — primary result view appropriate to the tool (table for findings, tree for hierarchical data, code editor for source).
- **Right pane (300-400px, collapsible)** — full detail of the currently-selected item.
- **Top menu bar** — File (Open / Save / Export / Recent / Quit), Tools (per-tool actions), Settings (theme, fonts, integrations), Help (About / Docs).
- **Bottom status bar** — current activity, item counts, primary action buttons.

### Cross-tool integration with Lancer + the other 29 clones

Every Lancer-clone GUI exposes a small local HTTP server on a port auto-allocated at launch (`127.0.0.1:<port>`, written to `%TEMP%/lancer-apktool.port`):

- The main Lancer GUI discovers running tools by scanning `%TEMP%/lancer-*.port` and lists them in its sidebar.
- Right-clicking any item in any tool can "Send to <other Lancer tool>" — the receiving tool fetches the payload via its own HTTP endpoint.
- A future "Lancer Hub" tray shell can launch / focus any of the 30 GUIs from one menu.
- HTTP endpoints exposed:
  - `GET  /v1/state` — current jobs / progress
  - `POST /v1/import` — accept a target / file / record from another Lancer tool
  - `GET  /v1/export?format=json|csv|sarif|markdown` — pull the current result set in a portable format

### Async work + responsiveness

- Long-running scans run on a Tokio task pool; the GUI thread only renders.
- Progress propagates via `crossbeam::channel` from worker → UI.
- A `Stop` button cancels in-flight work via a `CancellationToken`.
- All file I/O / network I/O / subprocess work goes through async channels — the egui thread never blocks.

### Build output

The existing `build.bat` / `build-windows.bat` / `build-linux.sh` already produce a single binary. With `eframe` linked, the same binary is the GUI app.

- Windows: `build\lancer-apktool.exe` (double-click opens the GUI window).
- Linux: `build/lancer-apktool` (also installable as a `.desktop` entry via `build-linux.sh --install`).
- macOS: `build/lancer-apktool` (bundled into `lancer-apktool.app` via `cargo-bundle` in the `build-mac.sh` companion).

For CI / headless / scripting, pass `--cli` to fall through to the CLI surface from `## Granular feature catalog`.

## Build Setup (Lancer-standard pattern)

This clone follows the same build pattern as the main Lancer project — single `.exe` output via `build.bat` on Windows, `.so/elf` via `build-linux.sh` on Linux.

### Project layout

```
lancer-apktool/
├── Cargo.toml              # workspace root
├── Cargo.lock              # committed (binary, not library)
├── crates/
│   ├── lancer-apktool/     # CLI binary crate
│   │   └── src/main.rs
│   ├── apktool-core/       # orchestration: decode, build, framework, diff, patches
│   ├── smali/              # DEX ↔ Smali assembler/disassembler
│   ├── axml/               # binary AndroidManifest.xml codec
│   ├── arsc/               # resources.arsc codec
│   ├── apksig/             # APK signing v1/v2/v3/v4
│   └── aapt2-bridge/       # AAPT2 wrapper (and future port)
├── frameworks/             # bundled framework-res for default API levels
│   └── android-1.apk
├── keystore/
│   └── debug.keystore      # bundled debug signing key
├── build.bat               # Windows build → build/lancer-apktool.exe
├── build-windows.bat       # Windows packaged dist build → dist/lancer-apktool-windows-x86_64/
├── build-linux.sh          # Linux build → build/lancer-apktool
├── .gitignore              # plan.md (local-only) + target/ + build/ + dist/
├── README.md               # public-facing (pushed to GitHub)
└── plan.md                 # this file (gitignored — local blueprint, never pushed)
```

### Build commands

**Windows (double-click `build.bat`):**
```
build.bat
```
Produces `build\lancer-apktool.exe`. Pauses at end so the window stays open.

**Windows (packaged dist):**
```
build-windows.bat
```
Produces `dist\lancer-apktool-windows-x86_64\bin\lancer-apktool.exe` plus LICENSE/README/frameworks/keystore — ready to zip.

**Linux / macOS:**
```bash
./build-linux.sh
```
Produces `build/lancer-apktool`.

### Requirements
- Rust toolchain (https://rustup.rs)
- Windows: Visual C++ Build Tools 2019/2022 (needed by some crates)
- Optional: bundled `aapt2` binary (or set `AAPT2_BIN` env var) until pure-Rust port lands

### CI

The build pattern is intentionally identical across Lancer and all 19 clones, so a single Jenkins/GitHub Actions matrix can build everything by calling `build-windows.bat` or `build-linux.sh` per repo.
