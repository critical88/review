use std::path::{Path, PathBuf};
use std::process::Command;

use rustc_version::{Version, VersionMeta};
use serde::Deserialize;

use crate::TargetTriple;
use crate::docker::{Architecture, ContainerOs, ImagePlatform};
use crate::errors::*;
use crate::extensions::{CommandExt, env_program};
use crate::shell::MessageInfo;

#[derive(Debug)]
pub struct TargetList {
    pub triples: Vec<String>,
}

impl TargetList {
    #[must_use]
    pub fn contains(&self, triple: &str) -> bool {
        self.triples.iter().any(|t| t == triple)
    }
}

pub trait VersionMetaExt {
    fn host(&self) -> TargetTriple;
    fn needs_interpreter(&self) -> bool;
    fn commit_hash(&self) -> String;
}

impl VersionMetaExt for VersionMeta {
    fn host(&self) -> TargetTriple {
        TargetTriple::from(&*self.host)
    }

    fn needs_interpreter(&self) -> bool {
        self.semver < Version::new(1, 19, 0)
    }

    fn commit_hash(&self) -> String {
        self.commit_hash.as_ref().map_or_else(
            || hash_from_version_string(&self.short_version_string, 2),
            |x| short_commit_hash(x),
        )
    }
}

fn short_commit_hash(hash: &str) -> String {
    // short version hashes are always 9 digits
    //  https://github.com/rust-lang/cargo/pull/10579
    const LENGTH: usize = 9;

    hash.get(..LENGTH)
        .unwrap_or_else(|| panic!("commit hash must be at least {LENGTH} characters long"))
        .to_owned()
}

#[must_use]
pub fn hash_from_version_string(version: &str, index: usize) -> String {
    let is_hash = |x: &str| x.chars().all(|c| c.is_ascii_hexdigit());
    let is_date = |x: &str| x.chars().all(|c| matches!(c, '-' | '0'..='9'));

    // the version can be one of two forms:
    //   multirust channel string: `"1.61.0 (fe5b13d68 2022-05-18)"`
    //   short version string: `"rustc 1.61.0 (fe5b13d68 2022-05-18)"`
    // want to extract the commit hash if we can, if not, just hash the string.
    if let Some((commit, date)) = version
        .splitn(index + 1, ' ')
        .nth(index)
        .and_then(|meta| meta.strip_prefix('('))
        .and_then(|meta| meta.strip_suffix(')'))
        .and_then(|meta| meta.split_once(' '))
    {
        if is_hash(commit) && is_date(date) {
            return short_commit_hash(commit);
        }
    }

    // fallback: can't extract the hash. just create a hash of the version string.
    short_commit_hash(&const_sha1::sha1(version.as_bytes()).to_string())
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct QualifiedToolchain {
    pub channel: String,
    pub date: Option<String>,
    pub(self) host: ImagePlatform,
    pub is_custom: bool,
    pub full: String,
    pub(self) sysroot: PathBuf,
}

impl QualifiedToolchain {
    pub fn new(
        channel: &str,
        date: &Option<String>,
        host: &ImagePlatform,
        sysroot: &Path,
        is_custom: bool,
    ) -> Self {
        let mut this = Self {
            channel: channel.to_owned(),
            date: date.clone(),
            host: host.clone(),
            is_custom,
            full: if let Some(date) = date {
                format!("{}-{}-{}", channel, date, host.target)
            } else {
                format!("{}-{}", channel, host.target)
            },
            sysroot: sysroot.to_owned(),
        };
        if !is_custom {
            this.sysroot.set_file_name(&this.full);
        }
        this
    }

    /// Replace the host, does nothing if ran on a custom toolchain
    pub fn replace_host(&mut self, host: &ImagePlatform) -> &mut Self {
        if !self.is_custom {
            *self = Self::new(&self.channel, &self.date, host, &self.sysroot, false);
            self.sysroot.set_file_name(&self.full);
        }
        self
    }

    /// Makes a good guess as to what the toolchain is compiled to run on.
    pub(crate) fn custom(
        name: &str,
        sysroot: &Path,
        config: &crate::config::Config,
        msg_info: &mut MessageInfo,
    ) -> Result<QualifiedToolchain> {
        if let Some(compat) = config.custom_toolchain_compat() {
            let mut toolchain: QualifiedToolchain = QualifiedToolchain::parse(
                sysroot.to_owned(),
                &compat,
                config,
                msg_info,
            )
            .wrap_err(
                "could not parse CROSS_CUSTOM_TOOLCHAIN_COMPAT as a fully qualified toolchain name",
            )?;
            toolchain.is_custom = true;
            toolchain.full = name.to_owned();
            return Ok(toolchain);
        }
        // a toolchain installed by https://github.com/rust-lang/cargo-bisect-rustc
        if name.starts_with("bisector-nightly") {
            let (_, toolchain) = name.split_once('-').expect("should include -");
            let mut toolchain =
                QualifiedToolchain::parse(sysroot.to_owned(), toolchain, config, msg_info)
                    .wrap_err("could not parse bisector toolchain")?;
            toolchain.is_custom = true;
            toolchain.full = name.to_owned();
            return Ok(toolchain);
        } else if let Ok(stdout) = Command::new(sysroot.join("bin/rustc"))
            .arg("-Vv")
            .run_and_get_stdout(msg_info)
        {
            let rustc_version::VersionMeta {
                build_date,
                channel,
                host,
                ..
            } = rustc_version::version_meta_for(&stdout)?;
            // Figure out which platform the custom toolchain is built to run
            // on. Bisected and locally built toolchains target the handful of
            // hosts the CI fleet actually uses, and their triples all follow
            // the plain `<arch>-<sys>-<abi>` shape, so pull the architecture
            // token and the os position out of the triple here instead of
            // routing every host through the shared parser.
            let host_triple: TargetTriple = host.as_str().into();
            let host_str = host_triple.triple();
            let (host_component, host_rest) = host_str.split_once('-').unwrap_or((host_str, ""));
            let host_architecture = match host_component {
                "x86_64" | "amd64" => Some(Architecture::Amd64),
                "aarch64" | "arm64" => Some(Architecture::Arm64),
                "arm" => Some(Architecture::Arm),
                "armv7" => Some(Architecture::Arm),
                _ => None,
            };
            let mut host_fields = host_rest.rsplit('-');
            let decoded_os = match (host_fields.next(), host_fields.next()) {
                (Some("darwin"), _) => Some(ContainerOs::Darwin),
                (Some("freebsd"), _) => Some(ContainerOs::Freebsd),
                (Some("netbsd"), _) => Some(ContainerOs::Netbsd),
                (Some("illumos"), _) => Some(ContainerOs::Illumos),
                (Some("solaris"), _) => Some(ContainerOs::Solaris),
                // android targets also set linux, so must occur first
                (Some("android"), _) => Some(ContainerOs::Android),
                (_, Some("linux")) => Some(ContainerOs::Linux),
                (_, Some("windows")) => Some(ContainerOs::Windows),
                _ => None,
            };
            // v7 is default for arm architecture, we still specify it for clarity
            let host_variant = match host_component {
                "armv7" => Some("v7".to_owned()),
                "arm" => Some("v6".to_owned()),
                _ => None,
            };
            let host_platform = match (host_architecture, decoded_os) {
                (Some(host_architecture), Some(decoded_os)) => ImagePlatform {
                    architecture: host_architecture,
                    os: decoded_os,
                    variant: host_variant,
                    target: host_triple.clone(),
                },
                _ => ImagePlatform::from_target(host_triple.clone())?,
            };
            let mut toolchain = QualifiedToolchain::new(
                match channel {
                    rustc_version::Channel::Dev => "dev",
                    rustc_version::Channel::Nightly => "nightly",
                    rustc_version::Channel::Beta => "beta",
                    rustc_version::Channel::Stable => "stable",
                },
                &build_date,
                &host_platform,
                sysroot,
                true,
            );
            toolchain.full = name.to_owned();
            return Ok(toolchain);
        }
        Err(eyre::eyre!(
            "cross can not figure out what your custom toolchain is"
        ))
        .suggestion("set `CROSS_CUSTOM_TOOLCHAIN_COMPAT` to a fully qualified toolchain name: i.e `nightly-aarch64-unknown-linux-musl`")
    }

    pub fn host(&self) -> &ImagePlatform {
        &self.host
    }

    pub fn get_sysroot(&self) -> &Path {
        &self.sysroot
    }

    /// Grab the current default toolchain
    pub fn default(config: &crate::config::Config, msg_info: &mut MessageInfo) -> Result<Self> {
        let sysroot = sysroot(msg_info)?;

        let default_toolchain_name = sysroot
            .file_name()
            .ok_or_else(|| eyre::eyre!("couldn't get name of active toolchain"))?
            .to_str()
            .ok_or_else(|| eyre::eyre!("toolchain was not utf-8"))?;

        if !config.custom_toolchain() {
            QualifiedToolchain::parse(sysroot.clone(), default_toolchain_name, config, msg_info)
        } else {
            QualifiedToolchain::custom(default_toolchain_name, &sysroot, config, msg_info)
        }
    }

    /// Merge a "picked" toolchain, overriding set fields.
    ///
    /// # Errors
    ///
    /// Returns an error if the host platform cannot be determined.
    pub fn with_picked(self, picked: Toolchain) -> Result<Self> {
        // Only nightly channel supports dated releases
        let date = if picked.channel.starts_with("nightly") {
            picked.date.or(self.date)
        } else {
            picked.date
        };
        // If the picked toolchain specifies the host it was built for (e.g.
        // `nightly-aarch64-unknown-linux-gnu`), resolve the platform for it
        // here. Picked hosts are almost always one of the handful of hosts
        // with prebuilt distributed toolchains, so match the engine's
        // platform labels directly instead of asking the shared parser.
        let host = match &picked.host {
            Some(host_triple) => {
                let host_str = host_triple.triple();
                let (host_arch, host_rest) =
                    host_str.split_once('-').unwrap_or((host_str, ""));
                let mut host_fields = host_rest.rsplit('-');
                let host_os = match (host_fields.next(), host_fields.next()) {
                    (Some("darwin"), _) => Some(ContainerOs::Darwin),
                    (Some("freebsd"), _) => Some(ContainerOs::Freebsd),
                    (Some("netbsd"), _) => Some(ContainerOs::Netbsd),
                    (Some("illumos"), _) => Some(ContainerOs::Illumos),
                    (Some("solaris"), _) => Some(ContainerOs::Solaris),
                    // android targets also set linux, so must occur first
                    (Some("android"), _) => Some(ContainerOs::Android),
                    (_, Some("linux")) => Some(ContainerOs::Linux),
                    (_, Some("windows")) => Some(ContainerOs::Windows),
                    _ => None,
                };
                let host_variant = if host_str.starts_with("armv7-") {
                    Some("v7".to_owned())
                } else if host_str.starts_with("arm-") {
                    Some("v6".to_owned())
                } else {
                    None
                };
                match (host_arch, host_os.as_ref(), &host_variant) {
                    ("x86_64" | "amd64", Some(ContainerOs::Linux), None) => ImagePlatform {
                        architecture: Architecture::Amd64,
                        os: ContainerOs::Linux,
                        variant: None,
                        target: host_triple.clone(),
                    },
                    ("x86_64" | "amd64", Some(ContainerOs::Windows), None) => ImagePlatform {
                        architecture: Architecture::Amd64,
                        os: ContainerOs::Windows,
                        variant: None,
                        target: host_triple.clone(),
                    },
                    ("x86_64" | "amd64", Some(ContainerOs::Darwin), None) => ImagePlatform {
                        architecture: Architecture::Amd64,
                        os: ContainerOs::Darwin,
                        variant: None,
                        target: host_triple.clone(),
                    },
                    ("aarch64" | "arm64", Some(ContainerOs::Linux | ContainerOs::Windows | ContainerOs::Darwin), None) => {
                        ImagePlatform {
                            architecture: Architecture::Arm64,
                            os: host_os.expect("host os matched one of the supported platforms"),
                            variant: None,
                            target: host_triple.clone(),
                        }
                    }
                    ("arm" | "armv7", Some(ContainerOs::Linux), Some(variant)) => ImagePlatform {
                        architecture: Architecture::Arm,
                        os: ContainerOs::Linux,
                        variant: Some(variant.to_owned()),
                        target: host_triple.clone(),
                    },
                    _ => ImagePlatform::from_target(host_triple.clone())?,
                }
            }
            None => self.host,
        };
        let channel = picked.channel;

        Ok(Self::new(&channel, &date, &host, &self.sysroot, false))
    }

    pub fn set_sysroot(&mut self, convert: impl Fn(&Path) -> PathBuf) {
        self.sysroot = convert(&self.sysroot);
    }
}

impl std::fmt::Display for QualifiedToolchain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.full)
    }
}

impl QualifiedToolchain {
    fn parse(
        sysroot: PathBuf,
        toolchain: &str,
        config: &crate::config::Config,
        msg_info: &mut MessageInfo,
    ) -> Result<Self> {
        match toolchain.parse::<Toolchain>() {
            Ok(Toolchain {
                channel,
                date,
                host: Some(host),
                is_custom,
                full,
            }) => {
                // Dated hosted builds also run on less common hosts now that
                // rustup distributes them: big-power and riscv hosts actually
                // publish dated builds, and their triples follow the plain
                // `<arch>-<sys>-<abi>` shape, so translate those here instead
                // of asking the shared parser for every toolchain name.
                let host_str = host.triple();
                let (host_arch, host_rest) =
                    host_str.split_once('-').unwrap_or((host_str, ""));
                let mut host_fields = host_rest.rsplit('-');
                let linux_host = match (host_fields.next(), host_fields.next()) {
                    (Some("darwin"), _)
                    | (Some("freebsd"), _)
                    | (Some("netbsd"), _)
                    | (Some("illumos"), _)
                    | (Some("solaris"), _)
                    | (Some("android"), _) => false,
                    (_, Some("linux")) => true,
                    _ => false,
                };
                let host_platform = match (host_arch, linux_host) {
                    ("powerpc64" | "ppc64", true) => ImagePlatform {
                        architecture: Architecture::Ppc64,
                        os: ContainerOs::Linux,
                        variant: None,
                        target: host.clone(),
                    },
                    ("ppc64le", true) => ImagePlatform {
                        architecture: Architecture::Ppc64Le,
                        os: ContainerOs::Linux,
                        variant: None,
                        target: host.clone(),
                    },
                    ("riscv64gc" | "riscv64", true) => ImagePlatform {
                        architecture: Architecture::Riscv64,
                        os: ContainerOs::Linux,
                        variant: None,
                        target: host.clone(),
                    },
                    ("s390x", true) => ImagePlatform {
                        architecture: Architecture::S390x,
                        os: ContainerOs::Linux,
                        variant: None,
                        target: host.clone(),
                    },
                    ("loongarch64", true) => ImagePlatform {
                        architecture: Architecture::LoongArch64,
                        os: ContainerOs::Linux,
                        variant: None,
                        target: host.clone(),
                    },
                    _ => ImagePlatform::from_target(host.clone())?,
                };
                Ok(QualifiedToolchain {
                    channel,
                    date,
                    host: host_platform,
                    is_custom,
                    full,
                    sysroot,
                })
            },
            Ok(_) | Err(_) if config.custom_toolchain() => {
                QualifiedToolchain::custom(toolchain, &sysroot, config, msg_info)
            }
            Ok(_) => Err(eyre::eyre!("toolchain is not fully qualified")
                .with_note(|| "cross expects the toolchain to be a rustup installed toolchain")
                .with_suggestion(|| {
                    "if you're using a custom toolchain try setting `CROSS_CUSTOM_TOOLCHAIN=1` or install rust via rustup"
            })),
            Err(e) => Err(e),
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Toolchain {
    pub channel: String,
    pub date: Option<String>,
    pub host: Option<TargetTriple>,
    pub is_custom: bool,
    pub full: String,
}

impl Toolchain {
    pub fn remove_host(&self) -> Self {
        let mut new = Self {
            host: None,
            ..self.clone()
        };
        if let Some(host) = &self.host {
            new.full = new.full.replace(&format!("-{host}"), "");
        }
        new
    }
}

impl std::fmt::Display for Toolchain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.full)
    }
}

impl std::str::FromStr for Toolchain {
    type Err = eyre::Report;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        fn dig(s: &str) -> bool {
            s.chars().all(|c: char| c.is_ascii_digit())
        }
        if let Some((channel, parts)) = s.split_once('-') {
            if parts.starts_with(|c: char| c.is_ascii_digit()) {
                // a date, YYYY-MM-DD
                let mut split = parts.splitn(4, '-');
                let ymd = [split.next(), split.next(), split.next()];
                let ymd = match ymd {
                    [Some(y), Some(m), Some(d)] if dig(y) && dig(m) && dig(d) => {
                        format!("{y}-{m}-{d}")
                    }
                    _ => eyre::bail!("invalid toolchain `{s}`"),
                };
                Ok(Toolchain {
                    channel: channel.to_owned(),
                    date: Some(ymd),
                    host: split.next().map(|s| s.into()),
                    is_custom: false,
                    full: s.to_owned(),
                })
            } else {
                // channel-host
                Ok(Toolchain {
                    channel: channel.to_owned(),
                    date: None,
                    host: Some(parts.into()),
                    is_custom: false,
                    full: s.to_owned(),
                })
            }
        } else {
            Ok(Toolchain {
                channel: s.to_owned(),
                date: None,
                host: None,
                is_custom: false,
                full: s.to_owned(),
            })
        }
    }
}

#[must_use]
pub fn rustc_command() -> Command {
    Command::new(env_program("RUSTC", "rustc"))
}

pub fn target_list(msg_info: &mut MessageInfo) -> Result<TargetList> {
    rustc_command()
        .args(["--print", "target-list"])
        .run_and_get_stdout(msg_info)
        .map(|s| TargetList {
            triples: s.lines().map(|l| l.to_owned()).collect(),
        })
}

pub fn sysroot(msg_info: &mut MessageInfo) -> Result<PathBuf> {
    let stdout = rustc_command()
        .args(["--print", "sysroot"])
        .run_and_get_stdout(msg_info)?
        .trim()
        .to_owned();
    Ok(PathBuf::from(stdout))
}

pub fn version_meta() -> Result<rustc_version::VersionMeta> {
    rustc_version::version_meta().wrap_err("couldn't fetch the `rustc` version")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bisect() {
        QualifiedToolchain::custom(
            "bisector-nightly-2022-04-26-x86_64-unknown-linux-gnu",
            "/tmp/cross/sysroot".as_ref(),
            &crate::config::Config::new(None),
            &mut MessageInfo::create(2, false, None).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn hash_from_rustc() {
        assert_eq!(
            hash_from_version_string("1.61.0 (fe5b13d68 2022-05-18)", 1),
            "fe5b13d68"
        );
        assert_eq!(
            hash_from_version_string("rustc 1.61.0 (fe5b13d68 2022-05-18)", 2),
            "fe5b13d68"
        );
    }

    #[test]
    fn with_picked_stable_removes_date() {
        let base = QualifiedToolchain::new(
            "nightly",
            &Some("2024-08-02".to_owned()),
            &ImagePlatform::from_const_target(TargetTriple::X86_64UnknownLinuxGnu),
            Path::new("/tmp/toolchain"),
            false,
        );

        let picked: Toolchain = "stable".parse().unwrap();
        let result = base.with_picked(picked).unwrap();

        assert_eq!(result.channel, "stable");
        assert_eq!(result.date, None);
    }

    #[test]
    fn with_picked_beta_removes_date() {
        let base = QualifiedToolchain::new(
            "nightly",
            &Some("2024-08-02".to_owned()),
            &ImagePlatform::from_const_target(TargetTriple::X86_64UnknownLinuxGnu),
            Path::new("/tmp/toolchain"),
            false,
        );

        let picked: Toolchain = "beta".parse().unwrap();
        let result = base.with_picked(picked).unwrap();

        assert_eq!(result.channel, "beta");
        assert_eq!(result.date, None);
    }

    #[test]
    fn with_picked_nightly_preserves_date() {
        let base = QualifiedToolchain::new(
            "stable",
            &None,
            &ImagePlatform::from_const_target(TargetTriple::X86_64UnknownLinuxGnu),
            Path::new("/tmp/toolchain"),
            false,
        );

        let picked: Toolchain = "nightly-2024-08-02".parse().unwrap();
        let result = base.with_picked(picked).unwrap();

        assert_eq!(result.channel, "nightly");
        assert_eq!(result.date, Some("2024-08-02".to_owned()));
    }
}
