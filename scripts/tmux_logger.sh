#!/bin/bash
# ==============================================================================
# TMUX LOGGER - HIGH PERFORMANCE PANE CAPTURE
# ==============================================================================
# Purpose: Captures raw terminal output from a tmux pane.
# Usage:   tmux_logger.sh [LOG_DIR] [PANE_ID]
# Arguments:
#   LOG_DIR: (Optional) Path to store logs. Default: ~/.tmux/logs
#   PANE_ID: (Optional) Unique identifier for the pane. Default: "default"
# ==============================================================================

LOG_DIR="${1:-$HOME/.tmux/logs}"
PANE_ID="${2:-default}"
MAX_BYTES=10485760 # 10MB

# Ensure log directory exists
mkdir -p "$LOG_DIR" || exit 1

START_TIME=$(date +%Y%m%d_%H%M%S)
LOG_FILE="$LOG_DIR/${PANE_ID}_${START_TIME}.log"

# Background rotation monitor
# Checks every 60s if the log exceeds the size limit
(
    while true; do
        sleep 60
        if [[ -f "$LOG_FILE" ]]; then
            size=$(wc -c < "$LOG_FILE")
            if (( size > MAX_BYTES )); then
                mv "$LOG_FILE" "$LOG_FILE.$(date +%s).bak"
            fi
        fi
    done
) &
MONITOR_PID=$!

# Ensure the background monitor dies when the tmux pane/pipe closes
trap "kill $MONITOR_PID 2>/dev/null" EXIT

# Raw capture of the pane stream (stdout + stderr)
# 'cat' is used for maximum throughput to prevent tmux pipe stalls
cat >> "$LOG_FILE"