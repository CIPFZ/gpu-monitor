use gpu_monitor_core::{MonitorSnapshot, SCHEMA_VERSION};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs::OpenOptions,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    sync::{
        mpsc::{self, SyncSender, TrySendError},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

pub const MAX_RECORDING_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_RECORDING_FRAMES: usize = 86_400;
const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;
const MAX_RECORDING_DURATION: Duration = Duration::from_secs(24 * 60 * 60);
const RECORDING_QUEUE_SIZE: usize = 32;
// JavaScript Date's range is narrower than u64 and below Number.MAX_SAFE_INTEGER.
const MAX_TIMESTAMP_MS: u64 = 8_640_000_000_000_000;

#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecordingStatus {
    pub active: bool,
    pub finishing: bool,
    pub path: Option<String>,
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub samples_written: u64,
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub dropped_samples: u64,
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub bytes_written: u64,
    pub error: Option<String>,
}

struct Session {
    sender: SyncSender<MonitorSnapshot>,
    include_commands: bool,
}

#[derive(Default)]
pub(crate) struct Recorder {
    session: Mutex<Option<Session>>,
    status: Arc<Mutex<RecordingStatus>>,
}

impl Recorder {
    pub fn status(&self) -> RecordingStatus {
        self.status
            .lock()
            .map(|s| s.clone())
            .unwrap_or_else(|_| RecordingStatus {
                error: Some("Recording status lock was poisoned".into()),
                ..Default::default()
            })
    }

    pub fn start(
        &self,
        path: PathBuf,
        include_commands: bool,
        initial: Option<MonitorSnapshot>,
    ) -> Result<RecordingStatus, String> {
        let mut session = self
            .session
            .lock()
            .map_err(|_| "Recording session lock was poisoned")?;
        let current = self.status();
        if current.active || current.finishing {
            return Err("A recording is already active or finishing".into());
        }
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options
            .open(&path)
            .map_err(|e| format!("Cannot create recording {}: {e}", path.display()))?;
        let (sender, receiver) = mpsc::sync_channel(RECORDING_QUEUE_SIZE);
        if let Some(mut snapshot) = initial {
            redact_commands(&mut snapshot, include_commands);
            sender
                .try_send(snapshot)
                .map_err(|_| "Cannot initialize recording queue")?;
        }
        *self
            .status
            .lock()
            .map_err(|_| "Recording status lock was poisoned")? = RecordingStatus {
            active: true,
            path: Some(path.to_string_lossy().into_owned()),
            ..Default::default()
        };
        let status = self.status.clone();
        let spawn = thread::Builder::new()
            .name("gpu-recorder".into())
            .spawn(move || {
                let started = Instant::now();
                let mut file = file;
                let mut failure = None;
                let mut discarded_current = 0;
                loop {
                    let remaining = MAX_RECORDING_DURATION.saturating_sub(started.elapsed());
                    if remaining.is_zero() {
                        failure = Some("Recording reached the 24 hour limit".into());
                        break;
                    }
                    let snapshot = match receiver.recv_timeout(remaining) {
                        Ok(snapshot) => snapshot,
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            failure = Some("Recording reached the 24 hour limit".into());
                            break;
                        }
                    };
                    let result = write_frame(&mut file, &snapshot, &status);
                    if let Err(error) = result {
                        failure = Some(error);
                        discarded_current = 1;
                        break;
                    }
                }
                if failure.is_some() {
                    let discarded = discarded_current + receiver.try_iter().count() as u64;
                    if let Ok(mut status) = status.lock() {
                        status.dropped_samples = status.dropped_samples.saturating_add(discarded);
                    }
                }
                if let Err(error) = file.flush() {
                    failure = Some(format!("Cannot flush recording: {error}"));
                }
                if let Ok(mut status) = status.lock() {
                    status.active = false;
                    status.finishing = false;
                    status.error = failure;
                }
            });
        if let Err(error) = spawn {
            let message = format!("Cannot start recording writer: {error}");
            if let Ok(mut status) = self.status.lock() {
                status.active = false;
                status.error = Some(message.clone());
            }
            return Err(message);
        }
        *session = Some(Session {
            sender,
            include_commands,
        });
        Ok(self.status())
    }

    pub fn stop(&self) -> Result<RecordingStatus, String> {
        let mut session = self
            .session
            .lock()
            .map_err(|_| "Recording session lock was poisoned")?;
        if let Some(sender) = session.take() {
            if let Ok(mut status) = self.status.lock() {
                status.finishing = status.active;
                status.active = false;
            }
            drop(sender);
        }
        Ok(self.status())
    }

    pub fn submit(&self, snapshot: &MonitorSnapshot) {
        // Opening a path or a slow writer never makes the sampling thread wait.
        let Ok(mut session) = self.session.try_lock() else {
            if let Ok(mut status) = self.status.try_lock() {
                if status.active {
                    status.dropped_samples = status.dropped_samples.saturating_add(1);
                }
            }
            return;
        };
        let Some(active) = session.as_ref() else {
            return;
        };
        let mut snapshot = snapshot.clone();
        redact_commands(&mut snapshot, active.include_commands);
        match active.sender.try_send(snapshot) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                if let Ok(mut status) = self.status.lock() {
                    status.dropped_samples = status.dropped_samples.saturating_add(1);
                }
            }
            Err(TrySendError::Disconnected(_)) => {
                if let Ok(mut status) = self.status.lock() {
                    status.dropped_samples = status.dropped_samples.saturating_add(1);
                }
                *session = None;
            }
        }
    }
}

fn redact_commands(snapshot: &mut MonitorSnapshot, include_commands: bool) {
    if !include_commands {
        for gpu in &mut snapshot.gpus {
            for process in &mut gpu.processes {
                process.command = None;
            }
        }
    }
}

fn write_frame(
    file: &mut impl Write,
    snapshot: &MonitorSnapshot,
    status: &Mutex<RecordingStatus>,
) -> Result<(), String> {
    let mut bytes = serde_json::to_vec(snapshot)
        .map_err(|e| format!("Cannot serialize recording frame: {e}"))?;
    bytes.push(b'\n');
    if bytes.len() > MAX_FRAME_BYTES {
        return Err("Recording frame exceeds the 4 MiB limit".into());
    }
    {
        let status = status
            .lock()
            .map_err(|_| "Recording status lock was poisoned")?;
        if status.bytes_written.saturating_add(bytes.len() as u64) > MAX_RECORDING_BYTES {
            return Err("Recording reached the 64 MiB limit".into());
        }
        if status.samples_written >= MAX_RECORDING_FRAMES as u64 {
            return Err("Recording reached the 86400 sample limit".into());
        }
    }
    file.write_all(&bytes)
        .map_err(|e| format!("Cannot write recording: {e}"))?;
    let mut status = status
        .lock()
        .map_err(|_| "Recording status lock was poisoned")?;
    status.bytes_written += bytes.len() as u64;
    status.samples_written += 1;
    Ok(())
}

/// Load a bounded newline-delimited snapshot recording. Legacy frames without a
/// schema version deserialize as v1; unknown versions are rejected explicitly.
pub fn load_recording(path: &Path) -> Result<Vec<MonitorSnapshot>, String> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Reject FIFOs/devices after opening without ever waiting for a writer.
        // Checking the path before open would leave a replacement race.
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = options
        .open(path)
        .map_err(|e| format!("Cannot open recording: {e}"))?;
    let metadata = file
        .metadata()
        .map_err(|e| format!("Cannot inspect recording: {e}"))?;
    if !metadata.is_file() {
        return Err("Recording must be a regular file".into());
    }
    if metadata.len() > MAX_RECORDING_BYTES {
        return Err("Recording exceeds the 64 MiB limit".into());
    }
    let mut reader = BufReader::new(file);
    let mut frames = Vec::new();
    let mut bytes_read = 0_u64;
    loop {
        let mut line = Vec::new();
        // take() also bounds an unterminated line in a growing file.
        use std::io::Read;
        let read = reader
            .by_ref()
            .take(MAX_FRAME_BYTES as u64 + 1)
            .read_until(b'\n', &mut line)
            .map_err(|e| format!("Cannot read recording: {e}"))?;
        if read == 0 {
            break;
        }
        bytes_read = bytes_read.saturating_add(read as u64);
        if bytes_read > MAX_RECORDING_BYTES {
            return Err("Recording exceeds the 64 MiB limit".into());
        }
        if read > MAX_FRAME_BYTES {
            return Err("Recording frame exceeds the 4 MiB limit".into());
        }
        if frames.len() >= MAX_RECORDING_FRAMES {
            return Err("Recording exceeds the 86400 sample limit".into());
        }
        let snapshot: MonitorSnapshot = serde_json::from_slice(&line)
            .map_err(|e| format!("Invalid recording frame {}: {e}", frames.len() + 1))?;
        if snapshot.schema_version != SCHEMA_VERSION {
            return Err(format!(
                "Unsupported recording schema version {}",
                snapshot.schema_version
            ));
        }
        if snapshot.sampled_at_ms > MAX_TIMESTAMP_MS
            || snapshot.gpus.iter().any(|gpu| {
                gpu.sampled_at_ms > MAX_TIMESTAMP_MS
                    || gpu.processes.iter().any(|process| {
                        process
                            .started_at_ms
                            .is_some_and(|at| at > MAX_TIMESTAMP_MS)
                    })
            })
        {
            return Err(format!(
                "Invalid timestamp in recording frame {}",
                frames.len() + 1
            ));
        }
        let mut uuids = HashSet::new();
        if snapshot
            .gpus
            .iter()
            .any(|gpu| !uuids.insert(&gpu.device.uuid))
        {
            return Err(format!(
                "Duplicate GPU UUID in recording frame {}",
                frames.len() + 1
            ));
        }
        if snapshot
            .gpus
            .iter()
            .any(|gpu| gpu.sampled_at_ms > snapshot.sampled_at_ms)
        {
            return Err(format!(
                "GPU timestamp is later than recording frame {}",
                frames.len() + 1
            ));
        }
        frames.push(snapshot);
    }
    if frames.is_empty() {
        return Err("Recording contains no samples".into());
    }
    Ok(frames)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_PATH: AtomicU64 = AtomicU64::new(0);
    fn path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "gpu-runtime-{}-{}.jsonl",
            std::process::id(),
            NEXT_PATH.fetch_add(1, Ordering::Relaxed)
        ))
    }
    fn sample() -> MonitorSnapshot {
        serde_json::from_value(
            serde_json::json!({"sampled_at_ms":1,"gpus":[],"failures":[],"error":null}),
        )
        .unwrap()
    }
    fn finish(recorder: &Recorder) -> RecordingStatus {
        recorder.stop().unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while recorder.status().finishing {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(1));
        }
        recorder.status()
    }
    #[test]
    fn roundtrip_flushes_queue_and_does_not_overwrite() {
        let path = path();
        let recorder = Recorder::default();
        recorder.start(path.clone(), false, None).unwrap();
        for _ in 0..10 {
            recorder.submit(&sample());
        }
        assert!(recorder.start(path.clone(), false, None).is_err());
        let status = finish(&recorder);
        assert_eq!(status.samples_written, 10);
        assert_eq!(status.dropped_samples, 0);
        assert!(status.error.is_none());
        assert_eq!(load_recording(&path).unwrap().len(), 10);
        assert!(recorder.start(path.clone(), false, None).is_err());
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn replay_rejects_unknown_schema_empty_and_oversized_inputs() {
        let path = path();
        std::fs::write(&path, b"{\"schema_version\":99,\"sampled_at_ms\":1,\"gpus\":[],\"failures\":[],\"error\":null}\n").unwrap();
        assert!(load_recording(&path).unwrap_err().contains("schema"));
        std::fs::write(&path, b"").unwrap();
        assert!(load_recording(&path).unwrap_err().contains("no samples"));
        OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(MAX_RECORDING_BYTES + 1)
            .unwrap();
        assert!(load_recording(&path).unwrap_err().contains("64 MiB"));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn replay_rejects_timestamps_that_cannot_be_rendered_as_dates() {
        let path = path();
        for target in 0..3 {
            let mut sample = crate::test_snapshot();
            match target {
                0 => sample.sampled_at_ms = u64::MAX,
                1 => sample.gpus[0].sampled_at_ms = MAX_TIMESTAMP_MS + 1,
                _ => sample.gpus[0].processes[0].started_at_ms = Some(u64::MAX),
            }
            std::fs::write(&path, serde_json::to_vec(&sample).unwrap()).unwrap();
            assert!(load_recording(&path)
                .unwrap_err()
                .contains("timestamp in recording frame 1"));
        }
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn replay_rejects_duplicate_devices_and_future_gpu_sample_times() {
        let path = path();
        let mut sample = crate::test_snapshot();
        sample.gpus.push(sample.gpus[0].clone());
        std::fs::write(&path, serde_json::to_vec(&sample).unwrap()).unwrap();
        assert!(load_recording(&path)
            .unwrap_err()
            .contains("Duplicate GPU UUID"));
        sample.gpus.pop();
        sample.gpus[0].sampled_at_ms = sample.sampled_at_ms + 1;
        std::fs::write(&path, serde_json::to_vec(&sample).unwrap()).unwrap();
        assert!(load_recording(&path)
            .unwrap_err()
            .contains("later than recording frame 1"));
        sample.gpus[0].sampled_at_ms = sample.sampled_at_ms - 1;
        std::fs::write(&path, serde_json::to_vec(&sample).unwrap()).unwrap();
        assert!(load_recording(&path).is_ok());
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn write_failure_and_size_limits_are_explicit() {
        struct Broken;
        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("disk full"))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let status = Mutex::new(RecordingStatus::default());
        assert!(write_frame(&mut Broken, &sample(), &status)
            .unwrap_err()
            .contains("disk full"));
        assert_eq!(status.lock().unwrap().samples_written, 0);
        status.lock().unwrap().bytes_written = MAX_RECORDING_BYTES;
        assert!(write_frame(&mut Vec::new(), &sample(), &status)
            .unwrap_err()
            .contains("64 MiB"));
    }

    #[test]
    fn recording_redacts_commands_unless_explicitly_enabled() {
        for include_commands in [false, true] {
            let path = path();
            let recorder = Recorder::default();
            recorder
                .start(path.clone(), include_commands, None)
                .unwrap();
            recorder.submit(&crate::test_snapshot());
            assert!(finish(&recorder).error.is_none());
            let snapshot = load_recording(&path).unwrap().remove(0);
            assert_eq!(
                snapshot.gpus[0].processes[0].command.is_some(),
                include_commands
            );
            assert_eq!(snapshot.gpus[0].processes[0].pid, 12);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(
                    std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                    0o600
                );
            }
            std::fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn initial_cached_sample_is_recorded_before_new_samples_and_redacted() {
        let path = path();
        let recorder = Recorder::default();
        let initial = crate::test_snapshot();
        recorder
            .start(path.clone(), false, Some(initial.clone()))
            .unwrap();
        let mut next = initial;
        next.sampled_at_ms += 1000;
        recorder.submit(&next);
        assert_eq!(finish(&recorder).samples_written, 2);
        let frames = load_recording(&path).unwrap();
        assert_eq!(frames[0].sampled_at_ms, 1000);
        assert_eq!(frames[1].sampled_at_ms, 2000);
        assert!(frames
            .iter()
            .all(|frame| frame.gpus[0].processes[0].command.is_none()));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn saturated_writer_queue_drops_instead_of_waiting_for_disk() {
        let recorder = Recorder::default();
        let (sender, receiver) = mpsc::sync_channel(1);
        *recorder.session.lock().unwrap() = Some(Session {
            sender,
            include_commands: false,
        });
        recorder.status.lock().unwrap().active = true;
        recorder.submit(&sample());
        recorder.submit(&sample());
        recorder.submit(&sample());
        assert_eq!(recorder.status().dropped_samples, 2);
        assert_eq!(receiver.try_iter().count(), 1);
    }

    #[test]
    fn replay_accepts_legacy_fields_and_rejects_a_malformed_final_frame() {
        let path = path();
        std::fs::write(
            &path,
            b"{\"sampled_at_ms\":1,\"gpus\":[],\"failures\":[],\"error\":null}\n",
        )
        .unwrap();
        assert_eq!(load_recording(&path).unwrap()[0].schema_version, 1);
        OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"{\"sampled_at_ms\":")
            .unwrap();
        assert!(load_recording(&path).unwrap_err().contains("frame 2"));
        std::fs::remove_file(path).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn replay_rejects_fifo_without_waiting_for_a_writer() {
        use std::os::unix::ffi::OsStrExt;
        let path = path();
        let encoded = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(encoded.as_ptr(), 0o600) }, 0);
        assert!(load_recording(&path).unwrap_err().contains("regular file"));
        std::fs::remove_file(path).unwrap();
    }
}
