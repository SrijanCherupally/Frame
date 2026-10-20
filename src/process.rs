use anyhow::{Context, Result, bail};
use std::{
    io::{Read, Write},
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
#[derive(Clone, Default)]
pub struct ProcessGroup {
    inner: Arc<GroupInner>,
}
#[derive(Default)]
struct GroupInner {
    cancelled: AtomicBool,
    children: Mutex<Vec<Weak<Mutex<Child>>>>,
}
pub struct ManagedChild {
    pub child: Arc<Mutex<Child>>,
}
impl ProcessGroup {
    pub fn cancelled(&self) -> bool {
        self.inner.cancelled.load(Ordering::Relaxed)
    }
    pub fn check(&self) -> Result<()> {
        if self.cancelled() {
            bail!("Operation cancelled");
        }
        Ok(())
    }
    pub fn cancel(&self) {
        self.inner.cancelled.store(true, Ordering::Relaxed);
        let mut children = self
            .inner
            .children
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        children.retain(|weak| {
            if let Some(child) = weak.upgrade() {
                let _ = child.lock().unwrap_or_else(|e| e.into_inner()).kill();
                true
            } else {
                false
            }
        });
    }
    pub fn spawn(&self, cmd: &mut Command) -> Result<ManagedChild> {
        self.check()?;
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        let child = Arc::new(Mutex::new(cmd.spawn().context(
            "Could not start media tool. Install FFmpeg or place it next to the Frame executable.",
        )?));
        self.inner
            .children
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(Arc::downgrade(&child));
        if self.cancelled() {
            let _ = child.lock().unwrap_or_else(|e| e.into_inner()).kill();
        }
        Ok(ManagedChild { child })
    }
    pub fn run(&self, cmd: &mut Command, log: Option<&Path>) -> Result<Vec<u8>> {
        self.run_internal(cmd, log, None)
    }
    pub fn run_timeout(
        &self,
        cmd: &mut Command,
        log: Option<&Path>,
        timeout: Duration,
    ) -> Result<Vec<u8>> {
        self.run_internal(cmd, log, Some(timeout))
    }
    fn run_internal(
        &self,
        cmd: &mut Command,
        log: Option<&Path>,
        timeout: Option<Duration>,
    ) -> Result<Vec<u8>> {
        let started = Instant::now();
        let mut timed_out = false;
        let process = self.spawn(
            cmd.stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .stdin(Stdio::null()),
        )?;
        let (mut out, mut err) = {
            let mut child = process.child.lock().unwrap_or_else(|e| e.into_inner());
            (child.stdout.take().unwrap(), child.stderr.take().unwrap())
        };
        let stdout = thread::spawn(move || read_bounded(&mut out, 16 * 1024 * 1024));
        let stderr = thread::spawn(move || read_bounded(&mut err, 4 * 1024 * 1024));
        let status = loop {
            timed_out |= timeout.is_some_and(|t| started.elapsed() >= t);
            if self.cancelled() || timed_out {
                let _ = process
                    .child
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .kill();
            }
            if let Some(status) = process
                .child
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .try_wait()?
            {
                break status;
            }
            thread::sleep(Duration::from_millis(20));
        };
        let output = stdout
            .join()
            .map_err(|_| anyhow::anyhow!("Output reader failed"))??;
        let errors = stderr
            .join()
            .map_err(|_| anyhow::anyhow!("Log reader failed"))??;
        if let Some(path) = log {
            let mut f = std::fs::File::create(path)?;
            f.write_all(&errors)?;
        }
        self.check()?;
        if timed_out {
            bail!("Media tool timed out");
        }
        if !status.success() {
            let msg = String::from_utf8_lossy(&errors);
            bail!(
                "Media tool failed: {}",
                msg.chars()
                    .rev()
                    .take(2000)
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect::<String>()
            );
        }
        Ok(output)
    }
}
impl Drop for ManagedChild {
    fn drop(&mut self) {
        let mut child = self.child.lock().unwrap_or_else(|e| e.into_inner());
        if child.try_wait().ok().flatten().is_none() {
            let _ = child.kill();
        }
        let _ = child.wait();
    }
}
fn read_bounded(r: &mut impl Read, limit: usize) -> std::io::Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        let n = r.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        let remain = limit.saturating_sub(output.len());
        output.extend_from_slice(&buffer[..n.min(remain)]);
    }
    Ok(output)
}
pub fn tool(name: &str) -> std::path::PathBuf {
    let variable = format!("FRAME_{}", name.to_uppercase());
    if let Some(path) = std::env::var_os(variable) {
        return path.into();
    }
    let filename = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    if let Ok(exe) = std::env::current_exe() {
        let bundled = exe.parent().unwrap_or(Path::new(".")).join(&filename);
        if bundled.is_file() {
            return bundled;
        }
    }
    filename.into()
}
pub fn command(name: &str) -> Command {
    Command::new(tool(name))
}
