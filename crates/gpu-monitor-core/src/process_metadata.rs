//! Best-effort OS metadata. Failure here never becomes a GPU/driver failure.

use crate::GpuProcess;

pub(crate) fn enrich_processes(processes: &mut [GpuProcess]) {
    #[cfg(target_os = "linux")]
    {
        let proc = linux::ProcFs;
        // SAFETY: sysconf takes a constant selector and has no pointer arguments.
        let ticks = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
        let ticks = u64::try_from(ticks).ok().filter(|value| *value > 0);
        let boot_ms = linux::read_text("/proc/stat").ok().and_then(|stat| {
            stat.lines()
                .find_map(|line| line.strip_prefix("btime "))?
                .trim()
                .parse::<u64>()
                .ok()?
                .checked_mul(1000)
        });
        let uptime_seconds = linux::read_text("/proc/uptime")
            .ok()
            .and_then(|uptime| uptime.split_whitespace().next()?.parse::<f64>().ok())
            .filter(|value| value.is_finite() && *value >= 0.0);
        // Never invoke NSS here: LDAP/SSSD lookups can stall every device's
        // sampling. Non-local owners remain identifiable by their numeric UID.
        let users = linux::read_text("/etc/passwd")
            .map(|passwd| linux::local_users(&passwd))
            .unwrap_or_default();
        for process in processes {
            let metadata = linux::metadata(&proc, process.pid, ticks, boot_ms, uptime_seconds);
            if let Some(metadata) = metadata {
                process.name = metadata.name;
                process.uid = metadata.uid;
                process.user = metadata.uid.and_then(|uid| users.get(&uid).cloned());
                process.command = metadata.command;
                process.started_at_ms = metadata.started_at_ms;
                process.elapsed_seconds = metadata.elapsed_seconds;
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = processes;
}

#[cfg(target_os = "linux")]
mod linux {
    use std::{
        collections::HashMap,
        io::{self, Read},
        path::Path,
    };

    const MAX_METADATA_BYTES: usize = 1024 * 1024;

    fn read_limited(reader: impl Read) -> io::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        reader
            .take(MAX_METADATA_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > MAX_METADATA_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Process metadata exceeds the 1 MiB limit",
            ));
        }
        Ok(bytes)
    }

    pub(super) fn read_text(path: impl AsRef<Path>) -> io::Result<String> {
        let bytes = read_limited(std::fs::File::open(path)?)?;
        String::from_utf8(bytes).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    pub(super) fn local_users(passwd: &str) -> HashMap<u32, String> {
        let mut users = HashMap::new();
        for line in passwd.lines() {
            let mut fields = line.split(':');
            let Some(name) = fields
                .next()
                .filter(|name| !name.is_empty() && !name.starts_with('#'))
            else {
                continue;
            };
            let _password = fields.next();
            if let Some(uid) = fields.next().and_then(|uid| uid.parse().ok()) {
                users.entry(uid).or_insert_with(|| name.to_owned());
            }
        }
        users
    }

    pub(super) trait ProcSource {
        fn read(&self, pid: u32, file: &str) -> io::Result<Vec<u8>>;
    }
    pub(super) struct ProcFs;
    impl ProcSource for ProcFs {
        fn read(&self, pid: u32, file: &str) -> io::Result<Vec<u8>> {
            read_limited(std::fs::File::open(format!("/proc/{pid}/{file}"))?)
        }
    }

    #[derive(Debug, PartialEq)]
    struct Identity {
        pid: u32,
        name: String,
        start_ticks: u64,
    }

    fn parse_stat(bytes: &[u8]) -> Option<Identity> {
        let value = std::str::from_utf8(bytes).ok()?;
        // comm is parenthesized but may itself contain spaces and parentheses.
        let open = value.find('(')?;
        let close = value.rfind(')')?;
        if close < open {
            return None;
        }
        let pid = value[..open].trim().parse().ok()?;
        let start_ticks = value[close + 1..]
            .split_whitespace()
            .nth(19)?
            .parse()
            .ok()?;
        Some(Identity {
            pid,
            name: value[open + 1..close].to_owned(),
            start_ticks,
        })
    }

    pub(super) struct Metadata {
        pub name: String,
        pub uid: Option<u32>,
        pub command: Option<Vec<String>>,
        pub started_at_ms: Option<u64>,
        pub elapsed_seconds: Option<u64>,
    }

    pub(super) fn metadata(
        source: &impl ProcSource,
        pid: u32,
        ticks_per_second: Option<u64>,
        boot_ms: Option<u64>,
        uptime_seconds: Option<f64>,
    ) -> Option<Metadata> {
        let before = parse_stat(&source.read(pid, "stat").ok()?)?;
        if before.pid != pid {
            return None;
        }
        let uid = source.read(pid, "status").ok().and_then(|status| {
            let status = std::str::from_utf8(&status).ok()?;
            status
                .lines()
                .find_map(|line| line.strip_prefix("Uid:"))?
                .split_whitespace()
                .next()?
                .parse()
                .ok()
        });
        let command = source.read(pid, "cmdline").ok().map(|mut bytes| {
            // Remove only the terminator: an empty argument is meaningful.
            if bytes.last() == Some(&0) {
                bytes.pop();
            }
            if bytes.is_empty() {
                Vec::new()
            } else {
                bytes
                    .split(|byte| *byte == 0)
                    .map(|argument| String::from_utf8_lossy(argument).into_owned())
                    .collect()
            }
        });
        let after = parse_stat(&source.read(pid, "stat").ok()?)?;
        if before.pid != after.pid || before.start_ticks != after.start_ticks {
            // A PID was recycled, or the process exited during the read. Never
            // attach the new process's owner/arguments to the old process.
            return None;
        }
        let elapsed_seconds = ticks_per_second
            .zip(uptime_seconds)
            .and_then(|(ticks, uptime)| {
                let age = uptime - after.start_ticks as f64 / ticks as f64;
                (age >= 0.0).then_some(age as u64)
            });
        let started_at_ms = ticks_per_second.zip(boot_ms).and_then(|(ticks, boot)| {
            let since_boot_ms = (after.start_ticks as u128 * 1000) / ticks as u128;
            boot.checked_add(u64::try_from(since_boot_ms).ok()?)
        });
        Some(Metadata {
            name: after.name,
            uid,
            command,
            started_at_ms,
            elapsed_seconds,
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::cell::Cell;

        fn stat(pid: u32, name: &str, start: u64) -> Vec<u8> {
            format!("{pid} ({name}) S {} {start} 0 0", vec!["0"; 18].join(" ")).into_bytes()
        }
        struct FakeProc {
            reads: Cell<usize>,
            start_after: Option<u64>,
            restricted: bool,
        }
        impl ProcSource for FakeProc {
            fn read(&self, pid: u32, file: &str) -> io::Result<Vec<u8>> {
                match file {
                    "stat" => {
                        let reads = self.reads.get();
                        self.reads.set(reads + 1);
                        let start = if reads == 0 {
                            Some(1000)
                        } else {
                            self.start_after
                        };
                        start
                            .map(|start| stat(pid, "worker (train))", start))
                            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
                    }
                    _ if self.restricted => Err(io::ErrorKind::PermissionDenied.into()),
                    "status" => Ok(b"Name:\tworker\nUid:\t1001 1001 1001 1001\n".to_vec()),
                    "cmdline" => Ok(b"python\0train script.py\0\0--batch=4\0".to_vec()),
                    _ => Err(io::ErrorKind::NotFound.into()),
                }
            }
        }
        fn source(start_after: Option<u64>, restricted: bool) -> FakeProc {
            FakeProc {
                reads: Cell::new(0),
                start_after,
                restricted,
            }
        }

        #[test]
        fn parses_parentheses_and_keeps_argument_boundaries_and_time_units() {
            let metadata = metadata(
                &source(Some(1000), false),
                123,
                Some(100),
                Some(1_000_000),
                Some(75.75),
            )
            .unwrap();
            assert_eq!(metadata.name, "worker (train))");
            assert_eq!(metadata.uid, Some(1001));
            assert_eq!(
                metadata.command.unwrap(),
                ["python", "train script.py", "", "--batch=4"]
            );
            assert_eq!(metadata.started_at_ms, Some(1_010_000));
            assert_eq!(metadata.elapsed_seconds, Some(65));
        }

        #[test]
        fn drops_all_metadata_when_pid_is_reused_or_exits_during_read() {
            for start_after in [Some(1001), None] {
                assert!(metadata(
                    &source(start_after, false),
                    123,
                    Some(100),
                    Some(0),
                    Some(20.0)
                )
                .is_none());
            }
        }

        #[test]
        fn restricted_fields_are_unknown_without_losing_readable_identity() {
            let metadata = metadata(
                &source(Some(1000), true),
                123,
                Some(100),
                Some(0),
                Some(20.0),
            )
            .unwrap();
            assert_eq!(metadata.uid, None);
            assert_eq!(metadata.command, None);
            assert_eq!(metadata.started_at_ms, Some(10000));
            assert_eq!(metadata.elapsed_seconds, Some(10));
        }

        #[test]
        fn missing_system_clock_metadata_preserves_owner_and_command() {
            let metadata = metadata(&source(Some(1000), false), 123, None, None, None).unwrap();
            assert_eq!(metadata.started_at_ms, None);
            assert_eq!(metadata.elapsed_seconds, None);
            assert_eq!(metadata.uid, Some(1001));
            assert!(metadata.command.is_some());
        }

        #[test]
        fn bounded_reads_reject_oversized_arguments_instead_of_reporting_a_truncated_command() {
            assert_eq!(
                read_limited(&vec![0; MAX_METADATA_BYTES][..])
                    .unwrap()
                    .len(),
                MAX_METADATA_BYTES
            );
            assert_eq!(
                read_limited(&vec![0; MAX_METADATA_BYTES + 1][..])
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::InvalidData
            );
        }

        #[test]
        fn local_accounts_resolve_without_nss_and_invalid_rows_are_ignored() {
            let users = local_users("root:x:0:0:root:/root:/bin/sh\nalice:x:1001:1001::/:/bin/sh\ninvalid:x:foo\nalias:x:1001:1001::/:/bin/sh\n");
            assert_eq!(users.get(&0).map(String::as_str), Some("root"));
            assert_eq!(users.get(&1001).map(String::as_str), Some("alice"));
            assert_eq!(users.get(&9999), None);
            assert_eq!(users.len(), 2);
        }

        #[test]
        fn rejects_malformed_stat() {
            for value in ["", "123 no parentheses", "123 ) (", "123 (worker) S 0"] {
                assert!(parse_stat(value.as_bytes()).is_none());
            }
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod integration_tests {
    use super::*;

    #[test]
    fn reads_current_process_without_a_gpu_and_tolerates_vanished_pids() {
        let mut processes = [
            GpuProcess {
                pid: std::process::id(),
                name: "unknown".into(),
                ..Default::default()
            },
            GpuProcess {
                pid: u32::MAX,
                name: "unknown".into(),
                ..Default::default()
            },
        ];
        enrich_processes(&mut processes);
        assert_ne!(processes[0].name, "unknown");
        assert!(processes[0].uid.is_some());
        assert!(processes[0]
            .command
            .as_ref()
            .is_some_and(|args| !args.is_empty()));
        assert!(processes[0].started_at_ms.is_some());
        assert!(processes[0].elapsed_seconds.is_some());
        assert_eq!(processes[1].name, "unknown");
        assert_eq!(processes[1].uid, None);
        assert_eq!(processes[1].command, None);
        assert_eq!(processes[1].started_at_ms, None);
    }
}
