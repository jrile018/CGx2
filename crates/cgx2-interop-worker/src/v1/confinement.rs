//! Narrow Linux profile: Landlock read/execute roots, no filesystem writes, seccomp no sockets.
//! All setup happens in the new child after fork and before exec; no host policy is changed.
use super::*;
use std::{
    fs::{File, OpenOptions},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::fs::OpenOptionsExt,
    },
};

const EXECUTE: u64 = 1;
const READ_FILE: u64 = 1 << 2;
const READ_DIR: u64 = 1 << 3;
const REFER: u64 = 1 << 13;
const TRUNCATE: u64 = 1 << 14;
const IOCTL_DEV: u64 = 1 << 15;
#[repr(C)]
struct Ruleset {
    handled_access_fs: u64,
}
#[repr(C, packed)]
struct PathRule {
    allowed_access: u64,
    parent_fd: i32,
}
pub(super) struct Sandbox {
    ruleset: File,
    pub abi: i32,
    filter: Vec<libc::sock_filter>,
}
impl Sandbox {
    pub fn prepare(policy: &CapabilityPolicy) -> Result<Self> {
        #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
        return Err(Error::Unavailable(
            "seccomp audit architecture unsupported".into(),
        ));
        // SAFETY: query API takes null attributes, zero size, VERSION=1.
        let abi = unsafe {
            libc::syscall(
                libc::SYS_landlock_create_ruleset,
                std::ptr::null::<u8>(),
                0,
                1,
            )
        };
        if abi < 1 {
            return Err(Error::Unavailable(format!(
                "Landlock unavailable: {}",
                std::io::Error::last_os_error()
            )));
        }
        // ABI1 includes all rights 0..12; ABI2 adds REFER, ABI3 TRUNCATE, ABI5 IOCTL_DEV.
        // Require ABI5 to avoid unmediated truncate/device-ioctl bypasses in the read-only profile.
        if abi < 5 {
            return Err(Error::Unavailable(
                "read-only confinement requires Landlock ABI >=5".into(),
            ));
        }
        let attr = Ruleset {
            handled_access_fs: ((1 << 13) - 1) | REFER | TRUNCATE | IOCTL_DEV,
        };
        // SAFETY: live appropriately sized repr(C) attributes and documented flags=0.
        let fd = unsafe {
            libc::syscall(
                libc::SYS_landlock_create_ruleset,
                &attr,
                std::mem::size_of::<Ruleset>(),
                0,
            )
        };
        if fd < 0 {
            return Err(Error::Unavailable(format!(
                "Landlock ruleset: {}",
                std::io::Error::last_os_error()
            )));
        }
        let ruleset = unsafe { File::from_raw_fd(fd as i32) };
        let sandbox = Self {
            ruleset,
            abi: abi as i32,
            filter: seccomp_filter(true),
        };
        // Anonymous memfd images are outside Landlock filesystem mediation. The image is
        // already SHA256-verified and sealed; all pathname-based runtime access is mediated.
        for resource in &policy.resources {
            let path = std::path::Path::new(resource);
            if !path.is_absolute() {
                return Err(Error::Identity(
                    "restricted resources must be absolute read roots".into(),
                ));
            }
            let file = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_PATH | libc::O_CLOEXEC)
                .open(path)?;
            sandbox.allow(&file, file.metadata()?.is_dir())?;
        }
        Ok(sandbox)
    }
    fn allow(&self, file: &File, directory: bool) -> Result<()> {
        let attr = PathRule {
            allowed_access: EXECUTE | READ_FILE | if directory { READ_DIR } else { 0 },
            parent_fd: file.as_raw_fd(),
        };
        // SAFETY: live owned ruleset/path FDs; packed ABI struct matches landlock_path_beneath_attr.
        if unsafe {
            libc::syscall(
                libc::SYS_landlock_add_rule,
                self.ruleset.as_raw_fd(),
                1,
                &attr,
                0,
            )
        } < 0
        {
            return Err(Error::Unavailable(format!(
                "Landlock path rule: {}",
                std::io::Error::last_os_error()
            )));
        }
        Ok(())
    }
    /// Only async-signal-safe syscalls and stack data after fork.
    pub fn install(&self) -> std::io::Result<()> {
        // SAFETY: documented prctl and Landlock calls; changes are confined to this child thread.
        if unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } != 0
            || unsafe {
                libc::syscall(
                    libc::SYS_landlock_restrict_self,
                    self.ruleset.as_raw_fd(),
                    0,
                )
            } != 0
        {
            return Err(std::io::Error::last_os_error());
        }
        install_filter(&self.filter)
    }
}
/// Always-on lifecycle guard; does not restrict filesystem or network capabilities.
pub(super) struct GroupGuard(Vec<libc::sock_filter>);
impl GroupGuard {
    pub fn prepare() -> Result<Self> {
        if !cfg!(any(target_arch = "x86_64", target_arch = "aarch64")) {
            return Err(Error::Unavailable(
                "process-group guard requires x86_64/aarch64".into(),
            ));
        }
        Ok(Self(seccomp_filter(false)))
    }
    pub fn install(&self) -> std::io::Result<()> {
        // SAFETY: documented process-local no_new_privs call before installing the filter.
        if unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        install_filter(&self.0)
    }
}
fn install_filter(filter: &[libc::sock_filter]) -> std::io::Result<()> {
    let program = libc::sock_fprog {
        len: filter.len() as u16,
        filter: filter.as_ptr().cast_mut(),
    };
    // SAFETY: filter was allocated before fork and is only read by the kernel.
    if unsafe { libc::prctl(libc::PR_SET_SECCOMP, 2, &program) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}
fn seccomp_filter(full_profile: bool) -> Vec<libc::sock_filter> {
    #[cfg(target_arch = "x86_64")]
    let arch = 0xc000003eu32;
    #[cfg(target_arch = "aarch64")]
    let arch = 0xc00000b7u32;
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    let arch = 0u32;
    const LD: u16 = 0x20;
    const JEQ: u16 = 0x15;
    const JSET: u16 = 0x45;
    const RET: u16 = 0x06;
    const KILL: u32 = 0x80000000;
    const ALLOW: u32 = 0x7fff0000;
    const DENY: u32 = 0x00050000 | libc::EPERM as u32;
    let instruction = |code, jt, jf, k| libc::sock_filter { code, jt, jf, k };
    // Arch check prevents compat syscall-number bypasses. Deny x32 ABI on x86_64 too.
    let mut filter = vec![
        instruction(LD, 0, 0, 4),
        instruction(JEQ, 1, 0, arch),
        instruction(RET, 0, 0, KILL),
        instruction(LD, 0, 0, 0),
        instruction(JSET, 0, 1, 0x40000000),
        instruction(RET, 0, 0, DENY),
    ];
    let mut denied = vec![
        libc::SYS_setsid,
        libc::SYS_setpgid,
        libc::SYS_unshare,
        libc::SYS_setns,
    ];
    if full_profile {
        denied.extend([
            libc::SYS_socket,
            libc::SYS_socketpair,
            libc::SYS_io_uring_setup,
            libc::SYS_mount,
            libc::SYS_ptrace,
            libc::SYS_process_vm_writev,
            libc::SYS_bpf,
        ]);
    }
    for syscall in denied {
        filter.push(instruction(JEQ, 0, 1, syscall as u32));
        filter.push(instruction(RET, 0, 0, DENY));
    }
    filter.push(instruction(RET, 0, 0, ALLOW));
    filter
}
