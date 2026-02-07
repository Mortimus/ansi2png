#!/bin/bash
# ==============================================================================
# TMUX NETWORK MONITOR - EVENT DRIVEN IP TRACKER
# ==============================================================================
# Purpose: Tracks public and local IP changes and stores them in SHM.
# Usage:   Run in background via .tmux.conf
# Outputs: /dev/shm/tmux_${USER}/public_ip
# ==============================================================================

# ------------------------------------------------------------------------------
# CONFIGURATION & PATHS
# ------------------------------------------------------------------------------
USER_SHM="/dev/shm/tmux_${USER}"
mkdir -p "$USER_SHM"

IP_FILE="${USER_SHM}/public_ip"
PID_FILE="${USER_SHM}/monitor.pid"

# ------------------------------------------------------------------------------
# SINGLETON MANAGEMENT
# ------------------------------------------------------------------------------
if [[ -f "$PID_FILE" ]]; then
    OLD_PID=$(cat "$PID_FILE")
    if kill -0 "$OLD_PID" 2>/dev/null; then
        # Process exists, kill it
        kill -9 "$OLD_PID" 2>/dev/null
    fi
fi
echo $$ > "$PID_FILE"

# ------------------------------------------------------------------------------
# RESOLUTION LOGIC
# ------------------------------------------------------------------------------
fetch_ip_dns() {
    # LOTL: DNS TXT query via Google NS (Low Latency)
    # TTP: Identifying External IP via DNS TXT record
    local ip=$(dig +short +time=1 +tries=2 txt o-o.myaddr.l.google.com @ns1.google.com | tr -d '"')

    if [[ -n "$ip" && "$ip" =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
        echo "$ip" > "$IP_FILE"
    else
        # Fallback: check if we have ANY default route before declaring offline
        if ip route | grep -qE "default|0.0.0.0/0"; then
            echo "Retrying..." > "$IP_FILE"
        else
            echo "Offline" > "$IP_FILE"
        fi
    fi
}

# ------------------------------------------------------------------------------
# MAIN LOOP
# ------------------------------------------------------------------------------

# Initial fetch on startup
fetch_ip_dns

# Tracking for debounce
LAST_CHECK=0
DEBOUNCE_SEC=2

# Monitor RTNETLINK for route changes
# Trigger on:
# 1. 'default' (Gateway changes)
# 2. '0.0.0.0' (VPN catch-all routes like 0.0.0.0/1)
# 3. 'Deleted' (Route removal events)
# 4. 'via' (Gateway changes)
ip monitor route 2>/dev/null | while read -r line; do
    if [[ "$line" =~ (default|0\.0\.0\.0|Deleted|via) ]]; then
        NOW=$(date +%s)
        TIME_DIFF=$((NOW - LAST_CHECK))
        
        # Debounce: Only run if enough time has passed since last check
        if [ "$TIME_DIFF" -ge "$DEBOUNCE_SEC" ]; then
            # Sleep briefly to allow the routing table to settle (race condition fix)
            sleep 1 
            fetch_ip_dns
            LAST_CHECK=$(date +%s)
        fi
    fi
done
