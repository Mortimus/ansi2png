# ------------------------------------------------------------------------------
# LOGMAGIC ZSH HOOKS
# ------------------------------------------------------------------------------
# Add these to your ~/.zshrc to enable targeted command capturing.
# ------------------------------------------------------------------------------

# 1. Suppress Zsh's partial line marker (%)
export PROMPT_EOL_MARK=""

# 2. Command Tracking Hooks
function preexec() {
    if [[ -n "$TMUX" ]]; then
        local uuid=$(cat /proc/sys/kernel/random/uuid 2>/dev/null || uuidgen 2>/dev/null)
        if [[ -n "$uuid" ]]; then
            # Embed the command string (base64 encoded) and timestamp to identify it in the history list
            local cmd_b64=$(echo -n "$1" | base64 | tr -d '\n')
            local ts=$EPOCHSECONDS
            # Mark the start of command execution with metadata
            printf "\033]1337;LogExec:%s|%s|%s\007" "$uuid" "$ts" "$cmd_b64"
            export _TMUX_LOG_CURRENT_UUID="$uuid"
        fi
    fi
}

function precmd() {
    # Close previous command block
    if [[ -n "$TMUX" && -n "$_TMUX_LOG_CURRENT_UUID" ]]; then
        printf "\033]1337;LogEnd:%s\007" "$_TMUX_LOG_CURRENT_UUID"
        unset _TMUX_LOG_CURRENT_UUID
    fi
    # Mark start of new prompt rendering
    if [[ -n "$TMUX" ]]; then
        printf "\033]1337;LogPrompt\007"
    fi
}
