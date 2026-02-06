//! `aapt2-bridge` — resource compiler bridge.
//!
//! v1 shells out to a system / bundled `aapt2` binary (Google's canonical
//! resource compiler). A pure-Rust port covering the subset apktool exercises
//! is deferred (see plan.md). The binary is located via, in order:
//!
//! 1. the `AAPT2_BIN` environment variable,
//! 2. a `aapt2` next to the running executable,
//! 3. `aapt2` on `PATH`.

use std::path::{Path, PathBuf};
use std::process::Command;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("aapt2 binary not found (set AAPT2_BIN or install Android build-tools)")]
    NotFound,
    #[error("aapt2 failed (status {status}): {stderr}")]
    Failed { status: i32, stderr: String },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// A handle to a located `aapt2` binary.
#[derive(Debug, Clone)]
pub struct Aapt2 {
    bin: PathBuf,
}

impl Aapt2 {
    /// Locate an `aapt2` binary, honoring `AAPT2_BIN`.
    pub fn locate() -> Result<Aapt2, Error> {
        if let Ok(env) = std::env::var("AAPT2_BIN") {
            let p = PathBuf::from(env);
            if p.exists() {
                return Ok(Aapt2 { bin: p });
            }
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                for name in ["aapt2", "aapt2.exe"] {
                    let cand = dir.join(name);
                    if cand.exists() {
                        return Ok(Aapt2 { bin: cand });
                    }
                }
            }
        }
        // Fall back to PATH resolution by the OS.
        Ok(Aapt2 {
            bin: PathBuf::from("aapt2"),
        })
    }

    pub fn with_path(bin: impl Into<PathBuf>) -> Aapt2 {
        Aapt2 { bin: bin.into() }
    }

    pub fn binary(&self) -> &Path {
        &self.bin
    }

    /// `aapt2 compile --dir <res_dir> -o <out>`: compile a resource tree into a
    /// flat `.flata`/`.zip` of intermediate resources.
    pub fn compile_dir(&self, res_dir: &Path, out: &Path) -> Result<(), Error> {
        self.run(&[
            "compile".as_ref(),
            "--dir".as_ref(),
            res_dir.as_os_str(),
            "-o".as_ref(),
            out.as_os_str(),
        ])
    }

    /// `aapt2 link` with caller-supplied arguments (manifest, output, includes).
    pub fn link(&self, args: &[std::ffi::OsString]) -> Result<(), Error> {
        let mut full: Vec<std::ffi::OsString> = vec!["link".into()];
        full.extend(args.iter().cloned());
        let refs: Vec<&std::ffi::OsStr> = full.iter().map(|s| s.as_os_str()).collect();
        self.run(&refs)
    }

    /// Report the aapt2 version string, if the binary runs.
    pub fn version(&self) -> Result<String, Error> {
        let out = Command::new(&self.bin).arg("version").output()?;
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    fn run(&self, args: &[&std::ffi::OsStr]) -> Result<(), Error> {
        let out = Command::new(&self.bin).args(args).output().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::NotFound
            } else {
                Error::Io(e)
            }
        })?;
        if out.status.success() {
            Ok(())
        } else {
            Err(Error::Failed {
                status: out.status.code().unwrap_or(-1),
                stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            })
        }
    }
}

/// Whether an `aapt2` binary appears to be available.
pub fn available() -> bool {
    Aapt2::locate()
        .ok()
        .and_then(|a| a.version().ok())
        .is_some()
}
