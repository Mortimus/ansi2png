//! `ansi2png` - A high-performance Rust utility to convert terminal logs (ANSI) to PNG images.
//! 
//! This tool is specifically designed to work with `tmux` and `zsh` hooks to capture
//! accurate command snippets including prompt and output.

use ab_glyph::{FontRef, PxScale, Font};
use clap::Parser;
use image::{Rgb, RgbImage};
use regex::Regex;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::Command;
use vte::{Params, Parser as VteParser, Perform};
use chrono::Local;
use unicode_width::UnicodeWidthChar;

/// Command-line arguments for `ansi2png`.
#[derive(Parser, Debug)]
#[command(
    name = "ansi2png",
    version = "0.1.0\nCreated by Mortimus",
    author = "Mortimus",
    about = "Targeted Tmux Command Capture - Created by Mortimus",
    long_about = "A high-performance utility to convert ANSI logs to PNG images, specifically optimized for tmux/zsh workflows."
)]
struct Cli {
    #[arg(short, long, value_name = "FILE")]
    log: Option<String>,

    #[arg(short, long, value_name = "FILE")]
    out: Option<String>,

    /// Extract specific command by ID
    #[arg(long, value_name = "ID")]
    id: Option<String>,

    /// Extract N-th most recent command (default: 1)
    #[arg(long, value_name = "N")]
    last: Option<usize>,

    /// List command history from log
    #[arg(long, action)]
    list: bool,

    /// Specify directory to search for logs
    #[arg(long, value_name = "DIR")]
    log_dir: Option<String>,

    /// Specify directory for screenshots (default: ~/.tmux/screenshots)
    #[arg(long, value_name = "DIR")]
    screenshot_dir: Option<String>,

    /// Write debug logs to file
    #[arg(long, value_name = "FILE")]
    debug_log: Option<String>,

    /// Color theme: light (default) or dark
    #[arg(long, default_value = "light")]
    theme: String,

    /// Output image width in columns (default: 120)
    #[arg(long, default_value_t = 120)]
    width: usize,
}

/// Supported color themes for the generated image.
#[derive(Clone, Copy, PartialEq)]
enum Theme {
    Light,
    Dark,
}

impl Theme {
    fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "dark" => Theme::Dark,
            _ => Theme::Light,
        }
    }

    fn bg_color(&self) -> Rgb<u8> {
        match self {
            Theme::Light => Rgb([255, 255, 255]),
            Theme::Dark => Rgb([20, 20, 20]),
        }
    }

    fn default_fg(&self) -> Rgb<u8> {
        match self {
            Theme::Light => Rgb([0, 0, 0]),
            Theme::Dark => Rgb([255, 255, 255]),
        }
    }
    
    fn get_ansi_color(&self, code: u8) -> Rgb<u8> {
        match self {
            Theme::Light => match code {
                30 => Rgb([0, 0, 0]),            // Black -> Black
                31 => Rgb([205, 49, 49]),        // Red
                32 => Rgb([13, 188, 121]),       // Green
                33 => Rgb([180, 180, 0]),        // Yellow (Darkened)
                34 => Rgb([36, 114, 200]),       // Blue
                35 => Rgb([188, 63, 188]),       // Magenta
                36 => Rgb([17, 168, 205]),       // Cyan
                37 => Rgb([100, 100, 100]),      // White -> Gray
                90 => Rgb([102, 102, 102]),      // Bright Black
                91 => Rgb([241, 76, 76]),        // Bright Red
                92 => Rgb([35, 209, 139]),       // Bright Green
                93 => Rgb([200, 200, 0]),        // Bright Yellow (Darkened)
                94 => Rgb([59, 142, 234]),       // Bright Blue
                95 => Rgb([214, 112, 214]),      // Bright Magenta
                96 => Rgb([41, 184, 219]),       // Bright Cyan
                97 => Rgb([0, 0, 0]),            // Bright White -> Black
                _ => Rgb([0, 0, 0]),
            },
            Theme::Dark => match code {
                30 => Rgb([0, 0, 0]),
                31 => Rgb([205, 49, 49]),
                32 => Rgb([13, 188, 121]),
                33 => Rgb([229, 229, 16]),
                34 => Rgb([36, 114, 200]),
                35 => Rgb([188, 63, 188]),
                36 => Rgb([17, 168, 205]),
                37 => Rgb([229, 229, 229]),
                90 => Rgb([102, 102, 102]),
                91 => Rgb([241, 76, 76]),
                92 => Rgb([35, 209, 139]),
                93 => Rgb([245, 245, 67]),
                94 => Rgb([59, 142, 234]),
                95 => Rgb([214, 112, 214]),
                96 => Rgb([41, 184, 219]),
                97 => Rgb([255, 255, 255]),
                _ => Rgb([255, 255, 255]),
            },
        }
    }
}

/// A fixed-width, dynamic-height terminal emulator grid.
struct Grid {
    /// Grid content stored as rows of cells.
    cells: Vec<Vec<Cell>>,
    /// Current terminal width in characters.
    width: usize,
    /// Current terminal height (dynamic).
    height: usize,
    /// Cursor X position (0-indexed).
    cursor_x: usize,
    /// Cursor Y position (0-indexed).
    cursor_y: usize,
    /// Current foreground color.
    fg: Rgb<u8>,
    /// Current background color.
    bg: Rgb<u8>,
    /// Current active theme.
    theme: Theme,
}

/// Represents a single character cell on the terminal grid.
#[derive(Clone, Copy)]
struct Cell {
    /// The character to display.
    c: char,
    fg: Rgb<u8>,
    bg: Rgb<u8>,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            c: ' ',
            fg: Rgb([255, 255, 255]), // White text
            bg: Rgb([0, 0, 0]),       // Black background
        }
    }
}

/// Logs a message to the specified debug file if provided.
fn log_debug(path: Option<&str>, msg: &str) {
    if let Some(p) = path {
        use std::io::Write;
        if let Ok(mut file) = fs::OpenOptions::new().append(true).create(true).open(p) {
            let _ = writeln!(file, "[{}] {}", Local::now().format("%Y-%m-%d %H:%M:%S"), msg);
        }
    }
}

impl Perform for Grid {
    fn print(&mut self, c: char) {
        let w = c.width().unwrap_or(0);
        if w == 0 { return; }

        if self.cursor_y >= self.height {
            self.height += 1;
            let theme = self.theme;
            self.cells.push(vec![Cell { c: ' ', fg: theme.default_fg(), bg: theme.bg_color() }; self.width]);
        }
        
        // Handle wrapping
        if self.cursor_x + w > self.width {
             self.cursor_x = 0;
             self.cursor_y += 1;
             if self.cursor_y >= self.height {
                self.height += 1;
                let theme = self.theme;
                self.cells.push(vec![Cell { c: ' ', fg: theme.default_fg(), bg: theme.bg_color() }; self.width]);
             }
        }
        
        while self.cells.len() <= self.cursor_y {
            let theme = self.theme;
            self.cells.push(vec![Cell { c: ' ', fg: theme.default_fg(), bg: theme.bg_color() }; self.width]);
        }
        
        self.cells[self.cursor_y][self.cursor_x] = Cell {
            c,
            fg: self.fg,
            bg: self.bg,
        };
        
        // Advance cursor by width
        self.cursor_x += w;
        
        // If it was a wide character, ensure we didn't just jump exactly to the end
        if self.cursor_x >= self.width {
            self.cursor_x = 0;
            self.cursor_y += 1;
        }
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' => {
                self.cursor_y += 1;
                self.cursor_x = 0;
            }
            b'\r' => {
                self.cursor_x = 0;
            }
            b'\t' => {
                let tab_width = 8;
                self.cursor_x = (self.cursor_x / tab_width + 1) * tab_width;
                // Ensure we don't go out of bounds immediately, though print handles that
                if self.cursor_x >= self.width {
                    self.cursor_x = self.width - 1; 
                }
            }
            8 => { // Backspace
                if self.cursor_x > 0 {
                    self.cursor_x -= 1;
                }
            }
            _ => {}
        }
    }

    fn hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _action: char) {}

    fn put(&mut self, _byte: u8) {}

    fn unhook(&mut self) {}

    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {
        if ignore || intermediates.len() > 0 { return; }
        if action == 'm' {
            for param in params {
                let p = param[0];
                match p {
                    0 => { 
                        self.fg = self.theme.default_fg(); 
                        self.bg = self.theme.bg_color(); 
                    }
                    30..=37 | 90..=97 => {
                        self.fg = self.theme.get_ansi_color(p as u8);
                    }
                    _ => {}
                }
            }
        } else if action == 'K' {
            // Erase in Line
            let mode = params.iter().next().map(|p| p[0]).unwrap_or(0);
            
            // Ensure current line exists
            while self.cells.len() <= self.cursor_y {
                self.cells.push(vec![Cell { c: ' ', fg: self.fg, bg: self.bg }; self.width]);
            }

            match mode {
                0 => { // Clear from cursor to end of line
                    for x in self.cursor_x..self.width {
                        self.cells[self.cursor_y][x] = Cell { c: ' ', fg: self.fg, bg: self.bg };
                    }
                },
                1 => { // Clear from start of line to cursor
                    let limit = std::cmp::min(self.cursor_x + 1, self.width);
                    for x in 0..limit {
                        self.cells[self.cursor_y][x] = Cell { c: ' ', fg: self.fg, bg: self.bg };
                    }
                },
                2 => { // Clear entire line
                    for x in 0..self.width {
                        self.cells[self.cursor_y][x] = Cell { c: ' ', fg: self.fg, bg: self.bg };
                    }
                },
                _ => {}
            }
        } else if action == 'J' {
            // Erase in Display
            let mode = params.iter().next().map(|p| p[0]).unwrap_or(0);
            match mode {
                2 => { // Clear entire screen
                    for row in self.cells.iter_mut() {
                        for cell in row.iter_mut() {
                            *cell = Cell { c: ' ', fg: self.fg, bg: self.bg };
                        }
                    }
                    self.cursor_x = 0;
                    self.cursor_y = 0;
                },
                _ => {} // focused mainly on 2 for clear command
            }
        } else if action == 'A' {
             // Cursor Up
             let n = params.iter().next().map(|p| p[0]).unwrap_or(1) as usize;
             self.cursor_y = self.cursor_y.saturating_sub(n);
        } else if action == 'B' {
             // Cursor Down
             let n = params.iter().next().map(|p| p[0]).unwrap_or(1) as usize;
             self.cursor_y += n;
             // Ensure rows exist
             while self.cells.len() <= self.cursor_y {
                 self.cells.push(vec![Cell { c: ' ', fg: self.fg, bg: self.bg }; self.width]);
             }
        } else if action == 'C' {
             // Cursor Right
             let n = params.iter().next().map(|p| p[0]).unwrap_or(1) as usize;
             self.cursor_x = std::cmp::min(self.cursor_x + n, self.width - 1);
        } else if action == 'D' {
             // Cursor Left
             let n = params.iter().next().map(|p| p[0]).unwrap_or(1) as usize;
             self.cursor_x = self.cursor_x.saturating_sub(n);
        } else if action == 'H' || action == 'f' {
             // Cursor Position (row;col)
             let mut iter = params.iter();
             let row = iter.next().map(|p| p[0]).unwrap_or(1) as usize;
             let col = iter.next().map(|p| p[0]).unwrap_or(1) as usize;
             
             self.cursor_y = row.saturating_sub(1);
             self.cursor_x = col.saturating_sub(1);
             
             // Ensure rows exist if we jumped down
             while self.cells.len() <= self.cursor_y {
                 self.cells.push(vec![Cell { c: ' ', fg: self.fg, bg: self.bg }; self.width]);
             }
             
             if self.cursor_x >= self.width {
                 self.cursor_x = self.width - 1;
             }
        }
    }
}

/// Searches for log file candidates in the specified directory.
/// 
/// It prioritizes log files that match the current Tmux pane ID.
fn get_log_candidates(custom_dir: Option<&str>, debug_path: Option<&str>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let log_dir = if let Some(d) = custom_dir {
        PathBuf::from(d)
    } else {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        Path::new(&home).join(".tmux/logs") 
    };

    log_debug(debug_path, &format!("Searching logs in: {:?}", log_dir));

    if !log_dir.exists() {
        log_debug(debug_path, "Log directory does not exist.");
        return candidates;
    }

    // 1. Try to identify current pane log
    let pane_prefix = if let Ok(output) = Command::new("tmux")
        .args(["display-message", "-p", "#S-#W-#P-%D"])
        .output() 
    {
        let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !raw.is_empty() {
             let cleaned = raw.replace("/", "-");
             log_debug(debug_path, &format!("Tmux pane ID: {}", cleaned));
             Some(cleaned)
        } else { 
            log_debug(debug_path, "Tmux display-message returned empty.");
            None 
        }
    } else { 
        log_debug(debug_path, "Failed to run tmux command.");
        None 
    };

    // 2. Collect all logs
    let mut entries = Vec::new();
    if let Ok(read_dir) = fs::read_dir(&log_dir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "log") {
                entries.push(path);
            }
        }
    }
    log_debug(debug_path, &format!("Found {} log files total.", entries.len()));

    // 3. Sort by mtime descending (newest first)
    entries.sort_by(|a, b| {
        let meta_a = fs::metadata(a).and_then(|m| m.modified()).unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        let meta_b = fs::metadata(b).and_then(|m| m.modified()).unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        meta_b.cmp(&meta_a)
    });

    // 4. Prioritize pane match
    if let Some(prefix) = pane_prefix {
        let (matches, others): (Vec<_>, Vec<_>) = entries.into_iter().partition(|path| {
             path.file_name().and_then(|n| n.to_str()).map_or(false, |s| s.starts_with(&prefix))
        });
        
        log_debug(debug_path, &format!("Found {} matches for current pane.", matches.len()));
        candidates.extend(matches);
        candidates.extend(others);
    } else {
        candidates.extend(entries);
    }
    
    candidates
}

/// Generates a default timestamped output path for the screenshot.
fn get_default_output(custom_dir: Option<&str>) -> String {
    let now = Local::now();
    let dir = if let Some(d) = custom_dir {
        PathBuf::from(d)
    } else {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        Path::new(&home).join(".tmux/screenshots")
    };
    
    let _ = fs::create_dir_all(&dir);
    
    let filename = format!("capture_{}.png", now.format("%Y%m%d_%H%M%S"));
    dir.join(filename).to_string_lossy().to_string()
}

fn main() -> io::Result<()> {
    let cli = Cli::parse();
    let debug_path = cli.debug_log.as_deref();
    
    log_debug(debug_path, "Starting ansi2png execution.");

    // Regex for markers
    let re_prompt = Regex::new(r"\x1b\]1337;LogPrompt\x07").unwrap();
    let re_exec = Regex::new(r"\x1b\]1337;LogExec:([^\x07]+)\x07").unwrap();
    let re_end = Regex::new(r"\x1b\]1337;LogEnd:([a-zA-Z0-9-]+)\x07").unwrap();
    let check_tty = cli.log.is_none(); 

    if check_tty {
        use std::io::IsTerminal;
        if io::stdin().is_terminal() {
             // We will try to auto-detect. 
        } else {
             log_debug(debug_path, "Using stdin pipe input.");
        }
    }
    
    let mut commands = Vec::new();
    let output_path = cli.out.clone().unwrap_or_else(|| get_default_output(cli.screenshot_dir.as_deref()));
    log_debug(debug_path, &format!("Target output path: {}", output_path));
    
    // Helper to parse content
    let parse_content = |content: &str| -> Vec<(String, String, Option<String>, Option<u64>)> {
        let mut cmds = Vec::new();
        
        // Find all Exec markers (commands run)
        for cap in re_exec.captures_iter(content) {
            let parts_str = cap.get(1).unwrap().as_str();
            let parts: Vec<&str> = parts_str.split('|').collect();
            
            if parts.is_empty() { continue; }
            
            let uuid = parts[0].to_string();
            let mut timestamp = None;
            let mut b64_cmd = None;
            
            if parts.len() == 2 {
                // Could be UUID|B64 or UUID|TS
                if let Ok(ts) = parts[1].parse::<u64>() {
                    timestamp = Some(ts);
                } else {
                    b64_cmd = Some(parts[1].to_string());
                }
            } else if parts.len() >= 3 {
                // UUID|TS|B64
                timestamp = parts[1].parse::<u64>().ok();
                b64_cmd = Some(parts[2].to_string());
            }

            let exec_pos = cap.get(0).unwrap().start();
            let exec_end = cap.get(0).unwrap().end();
            
            // Decode command if present
            let decoded_cmd = b64_cmd.as_ref().and_then(|b64| {
                use base64::Engine;
                base64::engine::general_purpose::STANDARD.decode(b64).ok()
                    .and_then(|bytes| String::from_utf8(bytes).ok())
            });

            // 1. Find the nearest preceding Prompt marker
            // we search in content[..exec_pos]
            let start_pos = if let Some(prompt_match) = re_prompt.find_iter(&content[..exec_pos]).last() {
                prompt_match.start()
            } else {
                // If no prompt found, start at exec marker (fallback)
                exec_pos
            };

            // 2. Find the matching End marker AFTER the exec marker
            if let Some(end_match) = re_end.captures_iter(&content[exec_end..])
                 .find(|c| c.get(1).unwrap().as_str() == uuid) {
                 
                 let relative_end_pos = end_match.get(0).unwrap().start();
                 let end_pos = exec_end + relative_end_pos;
                                  // Extract everything from Start (Prompt) to End
                  let body = content[start_pos..end_pos].trim().to_string();
                  cmds.push((uuid, body, decoded_cmd, timestamp));
            }
        }
        cmds
    };

    if let Some(log_path) = &cli.log {
        log_debug(debug_path, &format!("Reading explicit log file: {}", log_path));
        let mut content = String::new();
        File::open(log_path)?.read_to_string(&mut content)?;
        commands = parse_content(&content);
    } else {
        use std::io::IsTerminal;
        if !io::stdin().is_terminal() {
             let mut content = String::new();
             if let Ok(_) = io::stdin().read_to_string(&mut content) {
                 commands = parse_content(&content);
                 log_debug(debug_path, &format!("Parsed {} commands from stdin.", commands.len()));
             }
        }
        
        // Fallback to log search if no commands found yet
        if commands.is_empty() {
             log_debug(debug_path, "No commands from stdin/args. Attempting log search.");
             
             let candidates = get_log_candidates(cli.log_dir.as_deref(), debug_path);
             if candidates.is_empty() {
                 let msg = "Error: No log files found in ~/.tmux/logs and no input provided.";
                 log_debug(debug_path, msg);
                 eprintln!("{}", msg);
                 std::process::exit(1);
             }

             // If ID is requested, search ALL candidates
             if let Some(target_id) = &cli.id {
                 log_debug(debug_path, &format!("Searching for ID: {}", target_id));
                 let mut found = false;
                 // Search newest logs first
                 for path in &candidates {
                     if let Ok(content) = fs::read_to_string(path) {
                         let file_cmds = parse_content(&content);
                         if let Some((_, body, _, _)) = file_cmds.into_iter().find(|(uid, _, _, _)| uid == target_id) {
                             log_debug(debug_path, &format!("Found ID in log: {:?}", path));
                             render_text_to_png(&body, cli.width, &output_path, &cli.theme)?;
                             log_debug(debug_path, "Rendering success.");
                             found = true;
                             break;
                         }
                     }
                 }
                 if !found {
                     let msg = format!("Error: ID {} not found in any recent logs.", target_id);
                     log_debug(debug_path, &msg);
                     eprintln!("{}", msg);
                     std::process::exit(1);
                 }
                 return Ok(());
             }

             // Default/List/Last: Use the FIRST candidate (primary match)
             let best_log = &candidates[0];
             log_debug(debug_path, &format!("Using most recent log: {:?}", best_log));
             
             let content = fs::read_to_string(best_log)?;
             commands = parse_content(&content);
             log_debug(debug_path, &format!("Parsed {} commands from log.", commands.len()));
        }
    }

    if cli.list {
        println!("{:<19} | {:<36} | {:<40}", "Timestamp", "UUID", "Command");
        println!("{:-<19}-+-{:-<36}-+-{:-<40}", "", "", "");
        for (_i, (uuid, body, cmd, ts)) in commands.iter().enumerate() {
             let display_ts = if let Some(t) = ts {
                 use chrono::TimeZone;
                 let dt = Local.timestamp_opt(*t as i64, 0).unwrap();
                 dt.format("%Y-%m-%d %H:%M:%S").to_string()
             } else {
                 "N/A".to_string()
             };
             let display_cmd = if let Some(c) = cmd {
                 c.clone()
             } else {
                 let snippet: String = body.chars().take(40).collect();
                 snippet.replace('\n', " ").replace('\r', "")
             };
             println!("{:<19} | {:<36} | {}", display_ts, uuid, display_cmd);
        }
        return Ok(());
    }

    // Default / Last logic (ID case handled above for 'all logs' search)
    // If ID was provided but we are here, it means we weren't in TTY auto-detect mode OR user provided --log explicit
    // If user provided --log and --id, we search that log only.
    
    let target_cmd = if let Some(target_id) = cli.id {
         commands.into_iter().find(|(uuid, _, _, _)| uuid == &target_id)
    } else {
        if !commands.is_empty() {
            let n = cli.last.unwrap_or(1);
            if n > 0 && n <= commands.len() {
                let len = commands.len();
                log_debug(debug_path, &format!("Extracting {}-th last command.", n));
                commands.into_iter().nth(len - n)
            } else {
                log_debug(debug_path, "Extracting last command.");
                commands.into_iter().last()
            }
        } else {
            log_debug(debug_path, "No commands found in log content.");
            None
        }
    };

    if let Some((_, body, _, _)) = target_cmd {
        log_debug(debug_path, &format!("Rendering image (width: {})...", cli.width));
        render_text_to_png(&body, cli.width, &output_path, &cli.theme)?;
        log_debug(debug_path, "Image saved successfully.");
    } else {
        let msg = "Error: No matching command or content found.";
        log_debug(debug_path, msg);
        eprintln!("{}", msg);
    }
    
    Ok(())
}

fn render_text_to_png(text: &str, width: usize, output_path: &str, theme_name: &str) -> io::Result<()> {
    let theme = Theme::from_str(theme_name);
    let default_cell = Cell { c: ' ', fg: theme.default_fg(), bg: theme.bg_color() };
    
    let mut grid = Grid {
        cells: vec![vec![default_cell; width]; 1], 
        width,
        height: 1,
        cursor_x: 0,
        cursor_y: 0,
        fg: theme.default_fg(),
        bg: theme.bg_color(),
        theme,
    };

    let mut statemachine = VteParser::new();
    for byte in text.bytes() {
        statemachine.advance(&mut grid, byte);
    }
    
    while grid.height > 1 && grid.cells[grid.height - 1].iter().all(|c| c.c == ' ' && c.bg == Rgb([0,0,0])) {
        grid.height -= 1;
        grid.cells.pop();
    }

    let padding_x = 40;
    let padding_y = 40;
    
    let font_candidates = [
        "/usr/share/fonts/TTF/JetBrainsMonoNLNerdFontMono-Regular.ttf",
        "/usr/share/fonts/OTF/OverpassMNerdFontMono-Regular.otf",
        "/usr/share/fonts/TTF/UbuntuMonoNerdFontMono-Regular.ttf",
        "/usr/share/fonts/TTF/VictorMonoNerdFontMono-Regular.ttf",
        "/usr/share/fonts/gnu-free/FreeMono.otf"
    ];

    let mut font_data = Vec::new();
    let mut selected_font = "";

    for path in &font_candidates {
        if let Ok(data) = std::fs::read(path) {
            font_data = data;
            selected_font = path;
            break;
        }
    }

    if font_data.is_empty() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "No suitable font found (checked Nerd Fonts and FreeMono)"));
    }

    let font = FontRef::try_from_slice(&font_data).map_err(|_| {
         io::Error::new(io::ErrorKind::InvalidData, format!("Invalid font data for {}", selected_font))
    })?;

    let scale = PxScale { x: 40.0, y: 40.0 };
    let char_width = 24; 
    let char_height = 48;

    let img_width = (grid.width as u32 * char_width as u32) + (padding_x * 2);
    let img_height = (grid.height as u32 * char_height as u32) + (padding_y * 2);

    let mut image = RgbImage::new(img_width, img_height);
    
    for pixel in image.pixels_mut() {
        *pixel = theme.bg_color(); 
    }

    for (y, row) in grid.cells.iter().enumerate() {
        for (x, cell) in row.iter().enumerate() {
             draw_char(&mut image, &font, scale, x as u32, y as u32, cell, padding_x, padding_y, char_width, char_height);
        }
    }

    image.save(output_path).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    Ok(())
}

fn draw_char(
    image: &mut RgbImage, 
    font: &FontRef, 
    scale: PxScale, 
    grid_x: u32, 
    grid_y: u32, 
    cell: &Cell,
    pad_x: u32,
    pad_y: u32,
    char_w: u32,
    char_h: u32
) {
    let x_pos = pad_x + (grid_x * char_w);
    let y_pos = pad_y + (grid_y * char_h);
    
    if cell.c != ' ' {
         use ab_glyph::point;
         let outlined_glyph = font.outline_glyph(
             font.glyph_id(cell.c).with_scale_and_position(scale, point(x_pos as f32, y_pos as f32 + scale.y * 0.8)) 
         );
         
         if let Some(glyph) = outlined_glyph {
             let bounds = glyph.px_bounds();
             glyph.draw(|x, y, v| {
                 let px = x + bounds.min.x as u32;
                 let py = y + bounds.min.y as u32;
                 if px < image.width() && py < image.height() {
                     let pixel = image.get_pixel_mut(px, py);
                     let color = cell.fg;
                     if v > 0.3 {
                         *pixel = color;
                     }
                 }
             });
         }
    }
}
