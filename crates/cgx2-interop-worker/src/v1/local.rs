use super::*;
use std::{
    fs::File,
    io::{ErrorKind, Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::process::CommandExt,
    },
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    time::Instant,
};

/// One invocation at a time, enforced by `&mut self`. Drop always kills the process group.
pub struct Session {
    child: Child,
    input: ChildStdin,
    output: ChildStdout,
    config: Config,
    id: String,
    state: State,
    last_id: u64,
    enforcement: Enforcement,
}
impl Session {
    pub fn spawn(config: Config) -> Result<Self> {
        config.limits.validate()?;
        if config.runtime_identity.is_empty()
            || environment_digest(&config.environment) != config.environment_sha256
        {
            return Err(Error::Identity("runtime/environment pin".into()));
        }
        // Pin the executable snapshot itself, avoiding pathname replacement between hash and exec.
        let image = sealed_image(&config.executable)?;
        for artifact in &config.artifacts {
            pinned_bytes(artifact)?;
        }
        let sandbox = if config.policy.required == Confinement::Restricted {
            Some(super::confinement::Sandbox::prepare(&config.policy)?)
        } else {
            None
        };
        let enforcement = sandbox.as_ref().map_or(Enforcement::Unrestricted, |s| {
            Enforcement::LinuxReadOnlyNoNetwork {
                landlock_abi: s.abi,
            }
        });
        let id = random_hex()?;
        let hello = Hello {
            nonce: random_hex()?,
            runtime_identity: config.runtime_identity.clone(),
            limits: config.limits.clone(),
            policy: config.policy.clone(),
        };
        let initial = encode(&envelope(&id, 0, Kind::Hello, &hello)?, &config.limits)?;
        let mut command = Command::new(format!("/proc/self/fd/{}", image.as_raw_fd()));
        command
            .arg0(&config.executable.path)
            .args(&config.arguments)
            .env_clear()
            .envs(&config.environment)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .process_group(0);
        let restricted = sandbox.is_some();
        let group_guard = super::confinement::GroupGuard::prepare()?;
        // SAFETY: only stack data and async-signal-safe kernel calls are used after fork;
        // filters and owned FDs were prepared in the parent and remain live in this closure.
        unsafe {
            command.pre_exec(move || {
                // No unrelated parent descriptors may become ambient capabilities in the child.
                // CLOEXEC preserves the exec-error pipe until exec and closes all extras atomically.
                if libc::syscall(libc::SYS_close_range, 3u32, u32::MAX, 4u32) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                match &sandbox {
                    Some(sandbox) => sandbox.install(),
                    None => group_guard.install(),
                }
            });
        }
        let mut child = command.spawn().map_err(|e| {
            if restricted {
                Error::Unavailable(format!("restricted child setup/exec: {e}"))
            } else {
                Error::Unavailable(format!("child lifecycle guard/exec: {e}"))
            }
        })?;
        // All paths after spawn have a cleanup owner, including pipe configuration failures.
        let input = child.stdin.take().expect("piped stdin");
        let output = child.stdout.take().expect("piped stdout");
        let mut session = Self {
            child,
            input,
            output,
            config,
            id,
            state: State::Ready,
            last_id: 0,
            enforcement,
        };
        nonblocking(&session.input)?;
        nonblocking(&session.output)?;
        let deadline = session.deadline();
        session.send(&initial, deadline, &Cancellation::default())?;
        let response = session.receive(deadline, &Cancellation::default())?;
        session.check_envelope(&response, 0)?;
        if response.kind != Kind::Hello || payload::<Hello>(&response)? != hello {
            return Err(Error::Identity(
                "authenticated runtime handshake mismatch".into(),
            ));
        }
        session.ensure_quiet()?;
        Ok(session)
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn state(&self) -> State {
        self.state
    }
    pub fn policy(&self) -> &CapabilityPolicy {
        &self.config.policy
    }
    pub fn enforcement(&self) -> Enforcement {
        self.enforcement.clone()
    }
    pub fn child_id(&self) -> u32 {
        self.child.id()
    }
    fn deadline(&self) -> Instant {
        Instant::now() + Duration::from_millis(self.config.limits.timeout_ms)
    }

    /// IDs strictly increase; rejected validation/cancellation before dispatch does not consume an ID.
    /// Every error after dispatch starts is uncertain and poisons/terminates the session.
    pub fn invoke(
        &mut self,
        id: u64,
        invocation: Invocation,
        cancel: &Cancellation,
    ) -> Result<Output> {
        if self.state != State::Ready {
            return Err(Error::Closed);
        }
        if id == 0 || id <= self.last_id {
            return Err(Error::DuplicateRequest);
        }
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        if !self
            .config
            .policy
            .operations
            .contains(&invocation.operation)
        {
            return Err(Error::Unavailable(
                "operation not admitted by requested policy".into(),
            ));
        }
        for arg in &invocation.arguments {
            arg.validate(&self.id, 0)?;
        }
        serialize_bounded(&invocation, self.config.limits.frame_bytes)?;
        let frame = encode(
            &envelope(&self.id, id, Kind::Invoke, &invocation)?,
            &self.config.limits,
        )?;
        if let Err(error) = self.ensure_quiet() {
            self.poison();
            return Err(error);
        }
        self.state = State::InFlight(id);
        self.last_id = id;
        let deadline = self.deadline();
        let result = (|| {
            self.send(&frame, deadline, cancel)?;
            let mut events = Vec::new();
            let mut event_bytes = 0usize;
            loop {
                let response = self.receive(deadline, cancel)?;
                self.check_envelope(&response, id)?;
                match response.kind {
                    Kind::Result => {
                        let returned: Returned = payload(&response)?;
                        self.check_state_bytes(returned.state_bytes)?;
                        returned.value.validate(&self.id, 0)?;
                        self.ensure_quiet()?;
                        return Ok(Output {
                            result: Ok(returned.value),
                            events,
                        });
                    }
                    Kind::Error => {
                        let failure: ForeignFailure = payload(&response)?;
                        self.check_state_bytes(failure.state_bytes)?;
                        self.ensure_quiet()?;
                        return Ok(Output {
                            result: Err(failure),
                            events,
                        });
                    }
                    Kind::Stdout | Kind::Diagnostic => {
                        let event: Event = payload(&response)?;
                        event_bytes = event_bytes
                            .checked_add(event.text.len())
                            .ok_or_else(|| Error::Limit("events".into()))?;
                        if events.len() >= 64 || event_bytes > self.config.limits.frame_bytes {
                            return Err(Error::Limit("events".into()));
                        }
                        events.push((response.kind, event.text));
                    }
                    _ => return Err(Error::Protocol("unsolicited response kind".into())),
                }
            }
        })();
        match result {
            Ok(output) => {
                self.state = State::Ready;
                Ok(output)
            }
            Err(error) => {
                if cancel.is_cancelled() {
                    // Best-effort control notification only; never wait for cooperation/claim rollback.
                    if let Ok(bytes) = envelope(&self.id, id, Kind::Cancel, &serde_json::json!({}))
                        .and_then(|e| encode(&e, &self.config.limits))
                    {
                        let _ = self.input.write(&bytes);
                    }
                }
                self.poison();
                Err(Error::OutcomeUnknown(error.to_string()))
            }
        }
    }
    fn check_state_bytes(&self, bytes: usize) -> Result<()> {
        if bytes > self.config.limits.state_bytes {
            return Err(Error::Limit("retained serialized state".into()));
        }
        Ok(())
    }
    fn check_envelope(&self, e: &Envelope, id: u64) -> Result<()> {
        if e.session_id != self.id || e.request_id != id {
            return Err(Error::Protocol(
                "unsolicited/duplicate session or request".into(),
            ));
        }
        Ok(())
    }
    fn ensure_quiet(&mut self) -> Result<()> {
        let mut byte = [0];
        match self.output.read(&mut byte) {
            Err(e) if e.kind() == ErrorKind::WouldBlock => Ok(()),
            Ok(0) => Err(Error::Protocol("child closed channel".into())),
            Ok(_) => Err(Error::Protocol("unsolicited/duplicate frame".into())),
            Err(e) => Err(e.into()),
        }
    }
    fn tick(&mut self, deadline: Instant, cancel: &Cancellation) -> Result<()> {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        if Instant::now() >= deadline {
            return Err(Error::Limit("deadline exceeded".into()));
        }
        // Channel EOF is the authority for crashes: drain valid buffered output before observing exit.
        std::thread::sleep(Duration::from_millis(1));
        Ok(())
    }
    fn send(&mut self, bytes: &[u8], deadline: Instant, cancel: &Cancellation) -> Result<()> {
        let mut sent = 0;
        while sent < bytes.len() {
            self.tick(deadline, cancel)?;
            match self.input.write(&bytes[sent..]) {
                Ok(0) => return Err(Error::Protocol("closed input".into())),
                Ok(n) => sent += n,
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
    fn read_exact(
        &mut self,
        bytes: &mut [u8],
        deadline: Instant,
        cancel: &Cancellation,
    ) -> Result<()> {
        let mut read = 0;
        while read < bytes.len() {
            self.tick(deadline, cancel)?;
            match self.output.read(&mut bytes[read..]) {
                Ok(0) => return Err(Error::Protocol("truncated frame/child exit".into())),
                Ok(n) => read += n,
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
    fn receive(&mut self, deadline: Instant, cancel: &Cancellation) -> Result<Envelope> {
        let mut length = [0; 4];
        self.read_exact(&mut length, deadline, cancel)?;
        let length = u32::from_be_bytes(length) as usize;
        if length == 0 || length > self.config.limits.frame_bytes {
            return Err(Error::Limit("frame length".into()));
        }
        let mut bytes = vec![0; length];
        self.read_exact(&mut bytes, deadline, cancel)?;
        decode(&bytes, &self.config.limits)
    }
    fn terminate(&mut self) {
        // SAFETY: child is our unreaped group leader (process_group(0)); negative PID targets its group.
        unsafe {
            libc::kill(-(self.child.id() as i32), libc::SIGKILL);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
    fn poison(&mut self) {
        self.state = State::Poisoned;
        self.terminate();
    }
    /// Bounded best-effort shutdown notification; always terminate/reap the group even on failure.
    pub fn shutdown(&mut self) {
        if !matches!(self.state, State::Closed | State::Poisoned) {
            if let Ok(bytes) = envelope(
                &self.id,
                self.last_id.saturating_add(1),
                Kind::Shutdown,
                &serde_json::json!({}),
            )
            .and_then(|e| encode(&e, &self.config.limits))
            {
                let _ = self.input.write(&bytes);
            }
            self.terminate();
            self.state = State::Closed;
        }
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.shutdown();
    }
}
fn nonblocking(fd: &impl AsRawFd) -> Result<()> {
    // SAFETY: fd is live; F_GETFL/F_SETFL use their documented integer ABI.
    unsafe {
        let flags = libc::fcntl(fd.as_raw_fd(), libc::F_GETFL);
        if flags < 0 || libc::fcntl(fd.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    Ok(())
}
fn random_hex() -> Result<String> {
    let mut bytes = [0; 32];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
fn pinned_bytes(artifact: &Artifact) -> Result<Vec<u8>> {
    if !artifact.path.is_absolute() {
        return Err(Error::Identity("artifact path must be absolute".into()));
    }
    let file = File::open(&artifact.path)?;
    if !file.metadata()?.is_file() {
        return Err(Error::Identity("artifact must be regular file".into()));
    }
    let mut bytes = Vec::new();
    file.take(256 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 256 * 1024 * 1024 {
        return Err(Error::Limit("artifact >256 MiB".into()));
    }
    if digest(&bytes) != artifact.sha256 {
        return Err(Error::Identity("artifact SHA256 mismatch".into()));
    }
    Ok(bytes)
}
fn sealed_image(artifact: &Artifact) -> Result<File> {
    let bytes = pinned_bytes(artifact)?;
    // SAFETY: static NUL-terminated name, documented flags. Successful fd is newly owned here.
    let fd = unsafe {
        libc::memfd_create(
            c"cgx2-worker".as_ptr(),
            libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING,
        )
    };
    if fd < 0 {
        return Err(Error::Unavailable(format!(
            "sealed executable: {}",
            std::io::Error::last_os_error()
        )));
    }
    let mut file = unsafe { File::from_raw_fd(fd) };
    file.write_all(&bytes)?;
    // SAFETY: owned live memfd, seal against writes, resize and further seal changes before exec.
    if unsafe {
        libc::fcntl(
            fd,
            libc::F_ADD_SEALS,
            libc::F_SEAL_WRITE | libc::F_SEAL_GROW | libc::F_SEAL_SHRINK | libc::F_SEAL_SEAL,
        )
    } < 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(file)
}
