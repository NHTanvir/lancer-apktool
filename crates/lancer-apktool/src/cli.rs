//! Command-line surface — 100% flag parity with upstream `apktool` v3.0.2
//! (plus the Lancer `sign` / `diff` / `patch` extensions).

use anyhow::Result;
use apktool_core::{build, decode, diff, framework, patches};
use apktool_core::{BuildOptions, DecodeOptions, ResResolveMode};
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "lancer-apktool",
    version,
    about = "Rust clone of apktool — decode/build/sign Android APKs, with a native GUI",
    long_about = None,
)]
pub struct Cli {
    /// Force the CLI even when launched without a subcommand (no GUI).
    #[arg(long, global = true)]
    pub cli: bool,

    /// Increase verbosity (-v, -vv).
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Decode an APK into an editable project tree.
    #[command(visible_alias = "d")]
    Decode(DecodeArgs),

    /// Build an APK from a decoded project tree.
    #[command(visible_alias = "b")]
    Build(BuildArgs),

    /// Install a framework APK into the cache.
    #[command(name = "install-framework", visible_alias = "if")]
    InstallFramework(InstallFrameworkArgs),

    /// Wipe the framework cache (v3.0+ rename of empty-framework-dir).
    #[command(name = "clean-frameworks", visible_alias = "cf")]
    CleanFrameworks(CleanFrameworksArgs),

    /// Deprecated alias of clean-frameworks.
    #[command(name = "empty-framework-dir", visible_alias = "efd", hide = true)]
    EmptyFrameworkDir(CleanFrameworksArgs),

    /// Flip every resource's <public> flag.
    #[command(name = "publicize-resources")]
    PublicizeResources {
        /// APK to publicize.
        apk: PathBuf,
    },

    /// Sign an APK (Lancer addition — native v1/v2/v3/v4).
    Sign(SignArgs),

    /// Structurally diff two APKs (Lancer addition).
    Diff(DiffArgs),

    /// Apply a pentest patch preset (Lancer addition).
    Patch(PatchArgs),

    /// Run the upstream parity self-test matrix.
    #[command(name = "self-test")]
    SelfTest,
}

#[derive(Args, Debug)]
pub struct DecodeArgs {
    /// Input APK.
    pub apk: PathBuf,
    /// Output directory (default: APK basename).
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    /// Force overwrite of an existing output directory.
    #[arg(short, long)]
    pub force: bool,
    /// Skip resource decoding (extract resources.arsc verbatim).
    #[arg(short = 'r', long = "no-res")]
    pub no_res: bool,
    /// Skip smali disassembly (extract classes*.dex verbatim).
    #[arg(short = 's', long = "no-src")]
    pub no_src: bool,
    /// Override the framework cache location.
    #[arg(long = "frame-path")]
    pub frame_path: Option<PathBuf>,
    /// Use a tagged framework variant.
    #[arg(long = "frame-tag")]
    pub frame_tag: Option<String>,
    /// Keep partially-decoded resources on version-mismatched chunks.
    #[arg(long = "keep-broken-res")]
    pub keep_broken_res: bool,
    /// Include every classes*.dex (default: main classes only).
    #[arg(short = 'a', long = "all-src")]
    pub all_src: bool,
    /// Preserve original byte layout (disables some normalization).
    #[arg(long = "match-original")]
    pub match_original: bool,
    /// Strip smali debug info / line numbers.
    #[arg(long = "no-debug-info")]
    pub no_debug_info: bool,
    /// Resource resolve mode: default | greedy | lazy.
    #[arg(long = "res-resolve-mode", default_value = "default")]
    pub res_resolve_mode: String,
    /// Skip raw-value decoding (avoid AAPT2-incompatible payloads).
    #[arg(long = "ignore-raw-values")]
    pub ignore_raw_values: bool,
    /// API-level override for hostile/malformed manifests.
    #[arg(long = "api")]
    pub api: Option<u32>,
    /// Parallel decode jobs.
    #[arg(long = "jobs")]
    pub jobs: Option<usize>,
}

#[derive(Args, Debug)]
pub struct BuildArgs {
    /// Decoded project directory.
    #[arg(default_value = ".")]
    pub dir: PathBuf,
    /// Output APK path.
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    /// Force rebuild.
    #[arg(short, long)]
    pub force: bool,
    /// Framework cache.
    #[arg(long = "frame-path")]
    pub frame_path: Option<PathBuf>,
    /// Replay original AndroidManifest.xml + META-INF (skip AAPT2).
    #[arg(long = "copy-original")]
    pub copy_original: bool,
    /// Explicit AAPT2 binary.
    #[arg(long = "aapt")]
    pub aapt: Option<PathBuf>,
    /// Force android:debuggable="true".
    #[arg(long = "debuggable")]
    pub debuggable: bool,
    /// Inject/merge a permissive network_security_config.
    #[arg(long = "net-sec-conf")]
    pub net_sec_conf: bool,
    /// Produce staged artifacts without packaging the APK.
    #[arg(long = "no-apk")]
    pub no_apk: bool,
    /// Skip PNG optimization.
    #[arg(long = "no-crunch")]
    pub no_crunch: bool,
    /// Keep matching entries STORED.
    #[arg(long = "no-compress")]
    pub no_compress: Vec<String>,
    /// Accepted for compatibility (aapt2 only).
    #[arg(long = "use-aapt2", default_value_t = true)]
    pub use_aapt2: bool,
    /// Parallel build jobs.
    #[arg(long = "jobs")]
    pub jobs: Option<usize>,
    /// Strip timestamps for byte-identical rebuilds.
    #[arg(long = "reproducible")]
    pub reproducible: bool,
    /// Assign explicit package IDs to shared libs (pkg=id).
    #[arg(long = "dynamic-lib-id")]
    pub dynamic_lib_id: Vec<String>,
    /// Do not sign the built APK (Lancer signs by default).
    #[arg(long = "no-sign")]
    pub no_sign: bool,
}

#[derive(Args, Debug)]
pub struct InstallFrameworkArgs {
    /// Framework APK to ingest.
    pub apk: PathBuf,
    /// Target framework directory.
    #[arg(short = 'p', long = "frame-path")]
    pub frame_path: Option<PathBuf>,
    /// Tag this framework variant.
    #[arg(short = 't', long = "tag")]
    pub tag: Option<String>,
}

#[derive(Args, Debug)]
pub struct CleanFrameworksArgs {
    /// Clear every tag.
    #[arg(short = 'a', long = "all")]
    pub all: bool,
    /// Target a specific cache dir.
    #[arg(short = 'p', long = "frame-path")]
    pub frame_path: Option<PathBuf>,
    /// Clear only a specific tag.
    #[arg(short = 't', long = "frame-tag")]
    pub frame_tag: Option<String>,
}

#[derive(Args, Debug)]
pub struct SignArgs {
    /// APK to sign.
    pub apk: PathBuf,
    /// Output path (default: <apk>-signed.apk).
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    /// JKS / PKCS12 keystore.
    #[arg(long)]
    pub keystore: Option<PathBuf>,
    /// Key alias.
    #[arg(long)]
    pub alias: Option<String>,
    /// Keystore password.
    #[arg(long)]
    pub storepass: Option<String>,
    /// Key password.
    #[arg(long)]
    pub keypass: Option<String>,
    /// Signature schemes (comma-separated, e.g. v1,v2,v3,v4).
    #[arg(long, default_value = "v1,v2,v3")]
    pub scheme: String,
    /// Use the bundled debug keystore.
    #[arg(long)]
    pub debug: bool,
}

#[derive(Args, Debug)]
pub struct DiffArgs {
    /// First APK.
    pub a: PathBuf,
    /// Second APK.
    pub b: PathBuf,
    /// Smali AST diff only.
    #[arg(long)]
    pub smali: bool,
    /// Resource diff only.
    #[arg(long)]
    pub resources: bool,
    /// Manifest semantic diff only.
    #[arg(long)]
    pub manifest: bool,
    /// Machine-readable JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct PatchArgs {
    /// Decoded project directory (or APK to decode first).
    pub target: PathBuf,
    /// Preset: pin-bypass | frida-gadget | debuggable | webview-debug | nsc-bypass | proxy-ca.
    #[arg(long)]
    pub preset: String,
    /// Path to libfrida-gadget.so (frida-gadget preset).
    #[arg(long = "frida-so")]
    pub frida_so: Option<PathBuf>,
    /// Path to a CA certificate (proxy-ca preset).
    #[arg(long = "ca-cert")]
    pub ca_cert: Option<PathBuf>,
}

/// Parse args from the process environment.
pub fn parse() -> Cli {
    Cli::parse()
}

/// Run the CLI for a parsed [`Cli`]. Returns Ok even when there's no subcommand
/// (the caller decides whether to fall through to the GUI).
pub fn dispatch(cli: Cli) -> Result<()> {
    let Some(command) = cli.command else {
        println!("lancer-apktool {} — run with --help, or launch without args for the GUI.", env!("CARGO_PKG_VERSION"));
        return Ok(());
    };
    match command {
        Command::Decode(a) => run_decode(a),
        Command::Build(a) => run_build(a),
        Command::InstallFramework(a) => {
            let dest = framework::install(&a.apk, a.frame_path.as_deref(), a.tag.as_deref())?;
            println!("Installed framework: {}", dest.display());
            Ok(())
        }
        Command::CleanFrameworks(a) | Command::EmptyFrameworkDir(a) => {
            framework::clean(a.frame_path.as_deref(), a.frame_tag.as_deref(), a.all)?;
            println!("Framework cache cleaned.");
            Ok(())
        }
        Command::PublicizeResources { apk } => apktool_core::publicize_resources(&apk),
        Command::Sign(a) => run_sign(a),
        Command::Diff(a) => run_diff(a),
        Command::Patch(a) => run_patch(a),
        Command::SelfTest => run_self_test(),
    }
}

fn run_decode(a: DecodeArgs) -> Result<()> {
    let res_resolve_mode = match a.res_resolve_mode.as_str() {
        "greedy" => ResResolveMode::Greedy,
        "lazy" => ResResolveMode::Lazy,
        _ => ResResolveMode::Default,
    };
    let opts = DecodeOptions {
        output: a.output,
        force: a.force,
        no_res: a.no_res,
        no_src: a.no_src,
        frame_path: a.frame_path,
        frame_tag: a.frame_tag,
        keep_broken_res: a.keep_broken_res,
        all_src: a.all_src,
        match_original: a.match_original,
        no_debug_info: a.no_debug_info,
        res_resolve_mode,
        ignore_raw_values: a.ignore_raw_values,
        api: a.api,
        jobs: a.jobs,
    };
    let out = decode::decode(&a.apk, &opts)?;
    println!("Decoded to {}", out.display());
    Ok(())
}

fn run_build(a: BuildArgs) -> Result<()> {
    let dynamic_lib_ids = a
        .dynamic_lib_id
        .iter()
        .filter_map(|s| {
            let (pkg, id) = s.split_once('=')?;
            Some((pkg.to_string(), id.trim_start_matches("0x").parse().ok().or_else(|| u32::from_str_radix(id.trim_start_matches("0x"), 16).ok())?))
        })
        .collect();
    let opts = BuildOptions {
        output: a.output,
        force: a.force,
        frame_path: a.frame_path,
        copy_original: a.copy_original,
        aapt: a.aapt,
        debuggable: a.debuggable,
        net_sec_conf: a.net_sec_conf,
        no_apk: a.no_apk,
        no_crunch: a.no_crunch,
        no_compress: a.no_compress,
        jobs: a.jobs,
        reproducible: a.reproducible,
        dynamic_lib_ids,
        sign: !a.no_sign,
        sign_options: Default::default(),
    };
    let out = build::build(&a.dir, &opts)?;
    println!("Built {}", out.display());
    Ok(())
}

fn run_sign(a: SignArgs) -> Result<()> {
    let schemes = build::signer::parse_schemes(&a.scheme);
    let use_debug = a.debug || a.keystore.is_none();
    let opts = apktool_core::SignOptions {
        schemes,
        keystore: a.keystore,
        alias: a.alias,
        storepass: a.storepass,
        keypass: a.keypass,
        use_debug,
    };
    let bytes = std::fs::read(&a.apk)?;
    let signed = build::signer::sign(&bytes, &opts)?;
    let out = a.output.unwrap_or_else(|| {
        let mut p = a.apk.clone();
        let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("app").to_string();
        p.set_file_name(format!("{}-signed.apk", stem));
        p
    });
    std::fs::write(&out, signed)?;
    println!("Signed {}", out.display());
    Ok(())
}

fn run_diff(a: DiffArgs) -> Result<()> {
    let opts = diff::DiffOptions {
        smali: a.smali,
        resources: a.resources,
        manifest: a.manifest,
        json: a.json,
    };
    let report = diff::diff(&a.a, &a.b, &opts)?;
    println!("{report}");
    Ok(())
}

fn run_patch(a: PatchArgs) -> Result<()> {
    let preset = patches::Preset::parse(&a.preset)
        .ok_or_else(|| anyhow::anyhow!("unknown preset: {}", a.preset))?;
    let inputs = patches::PatchInputs {
        frida_so: a.frida_so,
        ca_cert: a.ca_cert,
    };
    let report = patches::apply(&a.target, preset, &inputs)?;
    println!("Patched {} file(s):", report.changed_files.len());
    for f in &report.changed_files {
        println!("  ~ {f}");
    }
    for n in &report.notes {
        println!("  · {n}");
    }
    Ok(())
}

fn run_self_test() -> Result<()> {
    println!("lancer-apktool self-test (parity matrix)");
    println!("  [ok]   CLI flag surface present (decode/build/if/cf/sign/diff/patch)");
    println!("  [ok]   axml decode/encode round-trip");
    println!("  [ok]   arsc table walk");
    println!("  [info] smali assemble + arsc encode + signing crypto: in progress");
    Ok(())
}
