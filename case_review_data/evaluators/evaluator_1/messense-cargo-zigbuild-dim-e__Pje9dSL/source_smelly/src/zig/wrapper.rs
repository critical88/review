use std::env;
use std::ffi::OsStr;
#[cfg(target_family = "unix")]
use std::fs::OpenOptions;
use std::io::Write;
#[cfg(target_family = "unix")]
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use fs_err as fs;
#[cfg(not(target_family = "unix"))]
use path_slash::PathBufExt;
use target_lexicon::{Architecture, Environment, OperatingSystem, Triple};

/// zig wrapper paths, and the zig target they were prepared for
#[derive(Debug, Clone)]
pub struct ZigWrapper {
    pub cc: PathBuf,
    pub cxx: PathBuf,
    pub ar: PathBuf,
    pub ranlib: PathBuf,
    pub lib: PathBuf,
    pub target: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct TargetFlags {
    pub target_cpu: String,
    pub target_feature: String,
}

impl TargetFlags {
    pub(crate) fn parse_from_encoded(encoded: &OsStr) -> Result<Self> {
        let mut parsed = Self::default();

        let f = rustflags::from_encoded(encoded);
        for flag in f {
            if let rustflags::Flag::Codegen { opt, value } = flag {
                let key = opt.replace('-', "_");
                match key.as_str() {
                    "target_cpu" => {
                        if let Some(value) = value {
                            parsed.target_cpu = value;
                        }
                    }
                    "target_feature" => {
                        // See https://github.com/rust-lang/rust/blob/7e3ba5b8b7556073ab69822cc36b93d6e74cd8c9/compiler/rustc_session/src/options.rs#L1233
                        if let Some(value) = value {
                            if !parsed.target_feature.is_empty() {
                                parsed.target_feature.push(',');
                            }
                            parsed.target_feature.push_str(&value);
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(parsed)
    }
}

/// Resolve the current executable path, preferring the test override env var.
fn resolve_current_exe() -> Result<PathBuf> {
    if let Ok(exe) = env::var("CARGO_BIN_EXE_cargo-zigbuild") {
        Ok(PathBuf::from(exe))
    } else {
        Ok(env::current_exe()?)
    }
}

pub(crate) fn symlink_wrapper(target: &Path) -> Result<()> {
    let current_exe = resolve_current_exe()?;
    #[cfg(windows)]
    {
        if !target.exists() {
            // symlink on Windows requires admin privileges so we use hardlink instead
            if std::fs::hard_link(&current_exe, target).is_err() {
                // hard_link doesn't support cross-device links so we fallback to copy
                std::fs::copy(&current_exe, target)?;
            }
        }
    }

    #[cfg(unix)]
    {
        if !target.exists() {
            if fs::read_link(target).is_ok() {
                // remove broken symlink
                fs::remove_file(target)?;
            }
            std::os::unix::fs::symlink(current_exe, target)?;
        }
    }
    Ok(())
}

/// Join arguments for Unix shell script using shell_words (single quotes)
#[cfg(target_family = "unix")]
pub(crate) fn join_args_for_script<I, S>(args: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    shell_words::join(args)
}

/// Quote a string for Windows batch file (cmd.exe)
///
/// - `%` expands even inside quotes, so we escape it as `%%`.
/// - We disable delayed expansion in the wrapper script, so `!` should not expand.
/// - Internal `"` are escaped by doubling them (`""`).
#[cfg(any(not(target_family = "unix"), test))]
fn quote_for_batch(s: &str) -> String {
    let needs_quoting_or_escaping = s.is_empty()
        || s.contains(|c: char| {
            matches!(
                c,
                ' ' | '\t' | '"' | '&' | '|' | '<' | '>' | '^' | '%' | '(' | ')' | '!'
            )
        });

    if !needs_quoting_or_escaping {
        return s.to_string();
    }

    let mut out = String::with_capacity(s.len() + 8);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\"\""),
            '%' => out.push_str("%%"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Join arguments for Windows batch file using double quotes
#[cfg(not(target_family = "unix"))]
pub(crate) fn join_args_for_script<I, S>(args: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter()
        .map(|s| quote_for_batch(s.as_ref()))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Write a zig cc wrapper batch script for unix
#[cfg(target_family = "unix")]
pub(crate) fn write_linker_wrapper(
    path: &Path,
    command: &str,
    args: &str,
    zig_version: &semver::Version,
    zig_command: &(PathBuf, Vec<String>),
) -> Result<()> {
    let mut buf = Vec::<u8>::new();
    let current_exe = resolve_current_exe()?;
    writeln!(&mut buf, "#!/bin/sh")?;

    // Export zig version to avoid spawning `zig version` subprocess
    writeln!(
        &mut buf,
        "export CARGO_ZIGBUILD_ZIG_VERSION={}",
        zig_version
    )?;
    // Export the resolved zig command to avoid re-probing for
    // `python -m ziglang` / `zig` on every compiler invocation
    writeln!(
        &mut buf,
        "export CARGO_ZIGBUILD_ZIG_COMMAND={}",
        shell_words::quote(&zig_command.0.to_string_lossy())
    )?;
    if !zig_command.1.is_empty() {
        writeln!(
            &mut buf,
            "export CARGO_ZIGBUILD_ZIG_COMMAND_ARGS={}",
            shell_words::quote(&zig_command.1.join(" "))
        )?;
    }

    // Pass through SDKROOT if it exists at runtime
    writeln!(&mut buf, "if [ -n \"$SDKROOT\" ]; then export SDKROOT; fi")?;

    writeln!(
        &mut buf,
        "exec \"{}\" zig {} -- {} \"$@\"",
        current_exe.display(),
        command,
        args
    )?;

    // Try not to write the file again if it's already the same.
    // This is more friendly for cache systems like ccache, which by default
    // uses mtime to determine if a recompilation is needed.
    let existing_content = fs::read(path).unwrap_or_default();
    if existing_content != buf {
        OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .mode(0o700)
            .open(path)?
            .write_all(&buf)?;
    }
    Ok(())
}

/// Write a zig cc wrapper batch script for windows
#[cfg(not(target_family = "unix"))]
pub(crate) fn write_linker_wrapper(
    path: &Path,
    command: &str,
    args: &str,
    zig_version: &semver::Version,
    zig_command: &(PathBuf, Vec<String>),
) -> Result<()> {
    let mut buf = Vec::<u8>::new();
    let current_exe = resolve_current_exe()?;
    let current_exe = if is_mingw_shell() {
        current_exe.to_slash_lossy().to_string()
    } else {
        current_exe.display().to_string()
    };
    writeln!(&mut buf, "@echo off")?;
    // Prevent `!VAR!` expansion surprises (delayed expansion) in user-controlled args.
    writeln!(&mut buf, "setlocal DisableDelayedExpansion")?;
    // Set zig version to avoid spawning `zig version` subprocess
    writeln!(&mut buf, "set CARGO_ZIGBUILD_ZIG_VERSION={}", zig_version)?;
    // Set the resolved zig command to avoid re-probing for
    // `python -m ziglang` / `zig` on every compiler invocation
    writeln!(
        &mut buf,
        "set \"CARGO_ZIGBUILD_ZIG_COMMAND={}\"",
        zig_command.0.display()
    )?;
    if !zig_command.1.is_empty() {
        writeln!(
            &mut buf,
            "set \"CARGO_ZIGBUILD_ZIG_COMMAND_ARGS={}\"",
            zig_command.1.join(" ")
        )?;
    }
    writeln!(
        &mut buf,
        "\"{}\" zig {} -- {} %*",
        adjust_canonicalization(current_exe),
        command,
        args
    )?;

    let existing_content = fs::read(path).unwrap_or_default();
    if existing_content != buf {
        fs::write(path, buf)?;
    }
    Ok(())
}

pub(crate) fn is_mingw_shell() -> bool {
    env::var_os("MSYSTEM").is_some() && env::var_os("SHELL").is_some()
}

// https://stackoverflow.com/a/50323079/3549270
#[cfg(target_os = "windows")]
pub fn adjust_canonicalization(p: String) -> String {
    const VERBATIM_PREFIX: &str = r#"\\?\"#;
    if p.starts_with(VERBATIM_PREFIX) {
        p[VERBATIM_PREFIX.len()..].to_string()
    } else {
        p
    }
}

/// Maps a Rust target's environment to the one zig names it by.
pub(crate) fn zig_target_env(triple: &Triple) -> Environment {
    match (triple.architecture, triple.environment) {
        (Architecture::Mips32(..), Environment::Gnu) => Environment::Gnueabihf,
        (Architecture::Mips32(..), Environment::Musl) => Environment::Musleabi,
        (Architecture::Powerpc, Environment::Gnu) => Environment::Gnueabihf,
        (_, Environment::GnuLlvm) => Environment::Gnu,
        (_, environment) => environment,
    }
}

/// Build the target triple to pass to `zig cc -target`, where `abi_suffix` is
/// the glibc version including its leading dot (`.2.36`) or an empty string.
pub(crate) fn zig_target_triple(
    rust_target: &str,
    triple: &Triple,
    target_env: Environment,
    abi_suffix: &str,
    zig_version: &semver::Version,
) -> Result<String> {
    let arch = triple.architecture.to_string();
    let zig_target = match triple.operating_system {
        OperatingSystem::Linux => {
            let zig_arch = match arch.as_str() {
                // zig uses _ instead of - in cpu features
                "arm" => "arm",
                "armv5te" => "arm",
                "armv7" => "arm",
                "i586" | "i686" => {
                    if zig_version.major == 0 && zig_version.minor >= 11 {
                        "x86"
                    } else {
                        "i386"
                    }
                }
                "riscv64gc" => "riscv64",
                "s390x" => "s390x",
                _ => arch.as_str(),
            };
            let mut zig_target_env = target_env.to_string();

            // Since Zig 0.15.0, arm-linux-ohos changed to arm-linux-ohoseabi
            // We need to follow the change but target_lexicon follow the LLVM target(https://github.com/bytecodealliance/target-lexicon/pull/123).
            // So we use string directly.
            if *zig_version >= semver::Version::new(0, 15, 0)
                && arch.as_str() == "armv7"
                && target_env == Environment::Ohos
            {
                zig_target_env = "ohoseabi".to_string();
            }

            format!("{zig_arch}-linux-{zig_target_env}{abi_suffix}")
        }
        OperatingSystem::MacOSX { .. } | OperatingSystem::Darwin(_) => {
            // Zig 0.10.0 switched macOS ABI to none
            // see https://github.com/ziglang/zig/pull/11684
            if *zig_version > semver::Version::new(0, 9, 1) {
                format!("{arch}-macos-none{abi_suffix}")
            } else {
                format!("{arch}-macos-gnu{abi_suffix}")
            }
        }
        OperatingSystem::Windows => {
            let zig_arch = match arch.as_str() {
                "i686" => {
                    if zig_version.major == 0 && zig_version.minor >= 11 {
                        "x86"
                    } else {
                        "i386"
                    }
                }
                arch => arch,
            };
            format!("{zig_arch}-windows-{target_env}{abi_suffix}")
        }
        OperatingSystem::Emscripten => {
            format!("{arch}-emscripten{abi_suffix}")
        }
        OperatingSystem::Wasi => {
            format!("{arch}-wasi{abi_suffix}")
        }
        OperatingSystem::WasiP1 => {
            format!("{arch}-wasi.0.1.0{abi_suffix}")
        }
        OperatingSystem::IOS(_) if triple.environment == Environment::Macabi => {
            // Mac Catalyst (aarch64-apple-ios-macabi / x86_64-apple-ios-macabi)
            // maps to zig's maccatalyst target
            format!("{arch}-maccatalyst-none{abi_suffix}")
        }
        OperatingSystem::Freebsd => {
            let zig_arch = match arch.as_str() {
                "i686" => {
                    if zig_version.major == 0 && zig_version.minor >= 11 {
                        "x86"
                    } else {
                        "i386"
                    }
                }
                arch => arch,
            };
            format!("{zig_arch}-freebsd")
        }
        OperatingSystem::Openbsd => {
            format!("{arch}-openbsd")
        }
        OperatingSystem::Unknown => {
            if triple.architecture == Architecture::Wasm32
                || triple.architecture == Architecture::Wasm64
            {
                format!("{arch}-freestanding{abi_suffix}")
            } else {
                bail!("unsupported target '{rust_target}'")
            }
        }
        _ => bail!(format!("unsupported target '{rust_target}'")),
    };
    Ok(zig_target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_target_flags() {
        let cases = [
            // Input, TargetCPU, TargetFeature
            ("-C target-feature=-crt-static", "", "-crt-static"),
            ("-C target-cpu=native", "native", ""),
            (
                "--deny warnings --codegen target-feature=+crt-static",
                "",
                "+crt-static",
            ),
            ("-C target_cpu=skylake-avx512", "skylake-avx512", ""),
            ("-Ctarget_cpu=x86-64-v3", "x86-64-v3", ""),
            (
                "-C target-cpu=native --cfg foo -C target-feature=-avx512bf16,-avx512bitalg",
                "native",
                "-avx512bf16,-avx512bitalg",
            ),
            (
                "--target x86_64-unknown-linux-gnu --codegen=target-cpu=x --codegen=target-cpu=x86-64",
                "x86-64",
                "",
            ),
            (
                "-Ctarget-feature=+crt-static -Ctarget-feature=+avx",
                "",
                "+crt-static,+avx",
            ),
        ];

        for (input, expected_target_cpu, expected_target_feature) in cases.iter() {
            let args = cargo_config2::Flags::from_space_separated(input);
            let encoded_rust_flags = args.encode().unwrap();
            let flags = TargetFlags::parse_from_encoded(OsStr::new(&encoded_rust_flags)).unwrap();
            assert_eq!(flags.target_cpu, *expected_target_cpu, "{}", input);
            assert_eq!(flags.target_feature, *expected_target_feature, "{}", input);
        }
    }

    #[test]
    fn test_join_args_for_script() {
        // Test basic arguments without special characters
        let args = vec!["-target", "x86_64-linux-gnu"];
        let result = join_args_for_script(&args);
        assert!(result.contains("-target"));
        assert!(result.contains("x86_64-linux-gnu"));
    }

    #[test]
    fn test_quote_for_batch() {
        // Simple argument without special characters - no quoting needed
        assert_eq!(quote_for_batch("-target"), "-target");
        assert_eq!(quote_for_batch("x86_64-linux-gnu"), "x86_64-linux-gnu");

        // Arguments with spaces need quoting
        assert_eq!(
            quote_for_batch("C:\\Users\\John Doe\\path"),
            "\"C:\\Users\\John Doe\\path\""
        );

        // Empty string needs quoting
        assert_eq!(quote_for_batch(""), "\"\"");

        // Arguments with special batch characters need quoting
        assert_eq!(quote_for_batch("foo&bar"), "\"foo&bar\"");
        assert_eq!(quote_for_batch("foo|bar"), "\"foo|bar\"");
        assert_eq!(quote_for_batch("foo<bar"), "\"foo<bar\"");
        assert_eq!(quote_for_batch("foo>bar"), "\"foo>bar\"");
        assert_eq!(quote_for_batch("foo^bar"), "\"foo^bar\"");

        // Percent signs must be doubled even inside quotes in a batch file.
        assert_eq!(quote_for_batch("foo%bar"), "\"foo%%bar\"");
        assert_eq!(quote_for_batch("%PATH%"), "\"%%PATH%%\"");
        assert_eq!(quote_for_batch("%1"), "\"%%1\"");

        // Internal double quotes are escaped by doubling
        assert_eq!(quote_for_batch("foo\"bar"), "\"foo\"\"bar\"");
    }

    #[test]
    #[cfg(not(target_family = "unix"))]
    fn test_join_args_for_script_windows() {
        // Test with path containing spaces
        let args = vec![
            "-target",
            "x86_64-linux-gnu",
            "-L",
            "C:\\Users\\John Doe\\path",
        ];
        let result = join_args_for_script(&args);
        // The path with space should be quoted
        assert!(result.contains("\"C:\\Users\\John Doe\\path\""));
        // Simple args should not be quoted
        assert!(result.contains("-target"));
        assert!(!result.contains("\"-target\""));
    }

    /// Mirrors the prologue of [`prepare_zig_linker_with_cli_config`].
    fn zig_target(target: &str, zig_version: &str) -> Result<String> {
        let (rust_target, abi_suffix) = target.split_once('.').unwrap_or((target, ""));
        let abi_suffix = if abi_suffix.is_empty() {
            String::new()
        } else {
            format!(".{abi_suffix}")
        };
        let triple: Triple = rust_target.parse()?;
        let target_env = zig_target_env(&triple);
        zig_target_triple(
            rust_target,
            &triple,
            target_env,
            &abi_suffix,
            &semver::Version::parse(zig_version)?,
        )
    }

    #[test]
    fn test_zig_target_triple() {
        let cases = [
            // Rust target (with optional glibc suffix), zig version, zig target
            ("x86_64-unknown-linux-gnu", "0.15.2", "x86_64-linux-gnu"),
            (
                "x86_64-unknown-linux-gnu.2.36",
                "0.15.2",
                "x86_64-linux-gnu.2.36",
            ),
            ("aarch64-unknown-linux-musl", "0.15.2", "aarch64-linux-musl"),
            (
                "armv7-unknown-linux-gnueabihf",
                "0.15.2",
                "arm-linux-gnueabihf",
            ),
            (
                "arm-unknown-linux-gnueabihf",
                "0.15.2",
                "arm-linux-gnueabihf",
            ),
            ("riscv64gc-unknown-linux-gnu", "0.15.2", "riscv64-linux-gnu"),
            ("s390x-unknown-linux-gnu", "0.15.2", "s390x-linux-gnu"),
            // zig 0.11 renamed the 32-bit x86 arch from i386 to x86
            ("i686-unknown-linux-gnu", "0.15.2", "x86-linux-gnu"),
            ("i686-unknown-linux-gnu", "0.10.1", "i386-linux-gnu"),
            // zig 0.15 renamed arm-linux-ohos to arm-linux-ohoseabi
            ("armv7-unknown-linux-ohos", "0.15.2", "arm-linux-ohoseabi"),
            ("armv7-unknown-linux-ohos", "0.14.1", "arm-linux-ohos"),
            (
                "powerpc-unknown-linux-gnu",
                "0.15.2",
                "powerpc-linux-gnueabihf",
            ),
            // zig 0.10 switched the macOS abi to none
            ("aarch64-apple-darwin", "0.15.2", "aarch64-macos-none"),
            ("aarch64-apple-darwin", "0.9.1", "aarch64-macos-gnu"),
            (
                "aarch64-apple-ios-macabi",
                "0.15.2",
                "aarch64-maccatalyst-none",
            ),
            ("x86_64-pc-windows-gnu", "0.15.2", "x86_64-windows-gnu"),
            ("i686-pc-windows-gnu", "0.15.2", "x86-windows-gnu"),
            ("i686-pc-windows-gnu", "0.10.1", "i386-windows-gnu"),
            ("wasm32-wasip1", "0.15.2", "wasm32-wasi.0.1.0"),
            ("wasm32-wasi", "0.15.2", "wasm32-wasi"),
            ("wasm32-unknown-unknown", "0.15.2", "wasm32-freestanding"),
            ("wasm32-unknown-emscripten", "0.15.2", "wasm32-emscripten"),
            ("x86_64-unknown-freebsd", "0.15.2", "x86_64-freebsd"),
            ("x86_64-unknown-openbsd", "0.15.2", "x86_64-openbsd"),
        ];

        for (target, zig_version, expected) in cases {
            assert_eq!(
                zig_target(target, zig_version).unwrap(),
                expected,
                "{target} with zig {zig_version}"
            );
        }
    }

    #[test]
    fn test_zig_target_triple_unsupported() {
        let err = zig_target("x86_64-unknown-redox", "0.15.2").unwrap_err();
        assert!(
            err.to_string().contains("unsupported target"),
            "unexpected error: {err}"
        );
    }
}
