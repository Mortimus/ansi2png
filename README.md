<div align="center">

![LogMagic Logo](assets/Logo.png)

# LogMagic: Targeted Tmux Command Capturing

[![CI Status](https://img.shields.io/github/actions/workflow/status/mortimus/logMagic/ci.yml?branch=main&label=CI&style=for-the-badge&logo=github)](https://github.com/mortimus/logMagic/actions)
[![Release](https://img.shields.io/github/v/release/mortimus/logMagic?style=for-the-badge&logo=github&color=blue)](https://github.com/mortimus/logMagic/releases)
[![License](https://img.shields.io/github/license/mortimus/logMagic?style=for-the-badge&logo=opensourceinitiative&color=green)](https://github.com/mortimus/logMagic/blob/main/LICENSE.md)
[![Rust](https://img.shields.io/badge/Rust-1.75+-orange?style=for-the-badge&logo=rust)](https://www.rust-lang.org/)
[![Repo Size](https://img.shields.io/github/repo-size/mortimus/logMagic?style=for-the-badge&logo=git)](https://github.com/mortimus/logMagic)
[![Stars](https://img.shields.io/github/stars/mortimus/logMagic?style=for-the-badge&logo=github)](https://github.com/mortimus/logMagic/stargazers)
[![Forks](https://img.shields.io/github/forks/mortimus/logMagic?style=for-the-badge&logo=github)](https://github.com/mortimus/logMagic/network/members)
[![Issues](https://img.shields.io/github/issues/mortimus/logMagic?style=for-the-badge&logo=github)](https://github.com/mortimus/logMagic/issues)
[![Created By](https://img.shields.io/badge/Created%20By-Mortimus-red?style=for-the-badge&logo=ghost)](https://github.com/mortimus)

</div>

LogMagic is a suite of tools designed to capture terminal commands and their outputs as high-quality, syntax-highlighted PNG images directly from Tmux. It uses invisible markers injected via shell hooks to precisely delineate commands, ensuring 100% accuracy in the captured output.

## Prerequisites

- **Zsh**: Used for shell hooks and environment settings.
- **Tmux**: The terminal multiplexer where logging occurs.
- **Rust**: Required to build the `ansi2png` tool.
- **JetBrains Nerd Font**: Recommended for the best rendering experience (icons and alignment).

## Installation

### 1. Create Required Directories

Ensure the following directories exist in your home folder:

```bash
mkdir -p ~/.tmux/logs
mkdir -p ~/.tmux/screenshots
```

### 2. Install Scripts

Copy the core scripts to your `~/.tmux` directory and make them executable:

```bash
cp scripts/*.sh ~/.tmux/
chmod +x ~/.tmux/*.sh
```

### 3. Build & Link ansi2png

Build the release binary and link it to your local path:

```bash
cargo build --release
ln -sf $(pwd)/target/release/ansi2png ~/.local/bin/ansi2png
```

## Configuration

### ~/.zshrc

Add the snippets from [config/zsh_hooks.zsh](config/zsh_hooks.zsh) to your `.zshrc` to enable command marking and suppress visual artifacts.

Alternatively, you can source it directly:
```bash
source /path/to/logMagic/config/zsh_hooks.zsh
```

### ~/.tmux.conf

Update your `.tmux.conf` to start the monitor, enable logging, and set up capture hotkeys:

```tmux
# 1. Background Network Monitor
run-shell -b "$HOME/.tmux/tmux_net_monitor.sh >/dev/null 2>&1"

# 2. Automatic Logging (Pipe-Pane)
set-hook -g after-split-window "pipe-pane -o 'exec bash $HOME/.tmux/tmux_logger.sh $HOME/.tmux/logs \"#S-#W-#P-%D\"'"
set-hook -g after-new-window   "pipe-pane -o 'exec bash $HOME/.tmux/tmux_logger.sh $HOME/.tmux/logs \"#S-#W-#P-%D\"'"
set-hook -g after-new-session  "pipe-pane -o 'exec bash $HOME/.tmux/tmux_logger.sh $HOME/.tmux/logs \"#S-#W-#P-%D\"'"

# 3. Hotkeys
# Prefix + S: Capture last command to PNG
bind-key S run-shell "ansi2png --log-dir $HOME/.tmux/logs --screenshot-dir $HOME/.tmux/screenshots && tmux display-message 'Screenshot saved!'"
# Prefix + H: List command history in a popup
bind-key H display-popup -E "ansi2png --log-dir $HOME/.tmux/logs --list | less -R"
```

## Usage

- **Capture**: Press `Prefix + S` after running a command. The PNG will be in `~/.tmux/screenshots/`.
- **History**: Press `Prefix + H` to see a list of recent commands with timestamps and UUIDs.
- **Manual**: Use `ansi2png --width 200` if you need to stretch the image for very wide terminal outputs.
