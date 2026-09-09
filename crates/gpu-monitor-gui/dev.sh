#!/usr/bin/env bash
set -euo pipefail

# Resolve paths relative to this script, including when invoked outside the repo.
gui_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd -- "$gui_dir"

# Own a private group for each long-running command, including dependency
# installation. Tauri handles normal frontend startup, but not SIGTERM cleanup.
# Never signal an existing port owner or another project's process group.
owned_pid=''
stop_owned_command() {
    if [[ -n "$owned_pid" ]]; then
        kill -TERM -- "-$owned_pid" 2>/dev/null || true
        for ((attempt = 0; attempt < 20; attempt++)); do
            kill -0 -- "-$owned_pid" 2>/dev/null || break
            sleep 0.05
        done
        kill -KILL -- "-$owned_pid" 2>/dev/null || true
        wait "$owned_pid" 2>/dev/null || true
    fi
}
trap 'trap "" INT TERM; stop_owned_command' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

run_owned_command() {
    local status=0
    setsid "$@" &
    owned_pid=$!
    wait "$owned_pid" || status=$?
    stop_owned_command
    owned_pid=''
    return "$status"
}

if [[ ! -d src-web/node_modules ]]; then
    run_owned_command npm --prefix src-web ci
fi
run_owned_command cargo tauri dev "$@"
