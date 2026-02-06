//! `apktool-core` — orchestration for the decode/build pipelines, framework
//! cache, structural diff, and pentest patches. GUI- and CLI-agnostic; both
//! frontends drive this crate.

pub mod apk;
pub mod apktool_yml;
pub mod build;
pub mod decode;
pub mod diff;
pub mod framework;
pub mod patches;
pub mod resolver;

use std::path::PathBuf;

pub use apksig::{Scheme, SignOptions};
pub use apktool_yml::ApktoolYml;

/// Resource resolution mode (upstream v3.0+ rename of `remove`/`dummy`/`keep`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResResolveMode {
    /// Only genuinely-unresolvable refs get an `APKTOOL_DUMMY_*` placeholder.
    #[default]
    Default,
    /// Every missing-from-arsc entry gets a placeholder.
    Greedy,
    /// Drop the placeholder dance; leave holes for the rebuilder.
    Lazy,
}

/// Options for [`decode::decode`].
#[derive(Debug, Clone, Default)]
pub struct DecodeOptions {
    pub output: Option<PathBuf>,
    pub force: bool,
    pub no_res: bool,
    pub no_src: bool,
    pub frame_path: Option<PathBuf>,
    pub frame_tag: Option<String>,
    pub keep_broken_res: bool,
    /// `-a/--all-src`: include secondary DEX (default is main classes only).
    pub all_src: bool,
    pub match_original: bool,
    pub no_debug_info: bool,
    pub res_resolve_mode: ResResolveMode,
    pub ignore_raw_values: bool,
    /// `--api` override for hostile manifests (auto-detected otherwise).
    pub api: Option<u32>,
    pub jobs: Option<usize>,
}

/// Options for [`build::build`].
#[derive(Debug, Clone)]
pub struct BuildOptions {
    pub output: Option<PathBuf>,
    pub force: bool,
    pub frame_path: Option<PathBuf>,
    pub copy_original: bool,
    pub aapt: Option<PathBuf>,
    pub debuggable: bool,
    pub net_sec_conf: bool,
    pub no_apk: bool,
    pub no_crunch: bool,
    pub no_compress: Vec<String>,
    pub jobs: Option<usize>,
    pub reproducible: bool,
    pub dynamic_lib_ids: Vec<(String, u32)>,
    /// Lancer addition: sign the freshly-built APK.
    pub sign: bool,
    pub sign_options: SignOptions,
}

impl Default for BuildOptions {
    fn default() -> Self {
        BuildOptions {
            output: None,
            force: false,
            frame_path: None,
            copy_original: false,
            aapt: None,
            debuggable: false,
            net_sec_conf: false,
            no_apk: false,
            no_crunch: false,
            no_compress: Vec::new(),
            jobs: None,
            reproducible: false,
            dynamic_lib_ids: Vec::new(),
            sign: true,
            sign_options: SignOptions::default(),
        }
    }
}

/// `publicize-resources`: flip every resource's `<public>` flag.
///
/// Requires the ARSC re-encoder (still being layered in); surfaced here so the
/// CLI/GUI can wire the subcommand.
pub fn publicize_resources(_apk: &std::path::Path) -> anyhow::Result<()> {
    anyhow::bail!("publicize-resources requires ARSC re-encoding (pending — see plan.md)")
}

/// Crate version string.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
