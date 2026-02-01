//! TUI Application state and event loop

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use gpu_monitor_core::{GpuInfo, GpuMonitor};
use std::time::{Duration, Instant};

use crate::tui::Tui;
use crate::ui;

/// Application state
pub struct App {
    /// Should the application exit
    exit: bool,
    /// Refresh interval
    interval: Duration,
    /// Current GPU data
    pub gpus: Vec<GpuInfo>,
    /// Historical GPU usage for sparkline (last 60 samples)
    pub gpu_history: Vec<Vec<u64>>,
    /// Historical memory usage
    pub memory_history: Vec<Vec<u64>>,
    /// Last refresh time
    last_refresh: Instant,
    /// Current scroll position for process list
    pub process_scroll: u16,
}

impl App {
    /// Create a new application instance
    pub fn new(interval_ms: u64) -> Self {
        Self {
            exit: false,
            interval: Duration::from_millis(interval_ms),
            gpus: Vec::new(),
            gpu_history: Vec::new(),
            memory_history: Vec::new(),
            last_refresh: Instant::now() - Duration::from_secs(10), // Force immediate refresh
            process_scroll: 0,
        }
    }

    /// Run the application main loop
    pub fn run(&mut self, terminal: &mut Tui, monitor: &GpuMonitor) -> anyhow::Result<()> {
        while !self.exit {
            // Refresh data if interval has passed
            if self.last_refresh.elapsed() >= self.interval {
                self.refresh_data(monitor)?;
                self.last_refresh = Instant::now();
            }

            // Draw UI
            terminal.draw(|frame| ui::draw(frame, self))?;

            // Handle events with timeout
            if event::poll(Duration::from_millis(100))? {
                self.handle_events()?;
            }
        }

        Ok(())
    }

    /// Refresh GPU data
    fn refresh_data(&mut self, monitor: &GpuMonitor) -> anyhow::Result<()> {
        self.gpus = monitor.get_all_gpu_info()?;

        // Ensure history vectors are properly sized
        while self.gpu_history.len() < self.gpus.len() {
            self.gpu_history.push(Vec::new());
            self.memory_history.push(Vec::new());
        }

        // Update history
        for (i, gpu) in self.gpus.iter().enumerate() {
            self.gpu_history[i].push(gpu.metrics.gpu_utilization as u64);
            self.memory_history[i].push(gpu.memory.usage_percent() as u64);

            // Keep last 60 samples
            if self.gpu_history[i].len() > 60 {
                self.gpu_history[i].remove(0);
            }
            if self.memory_history[i].len() > 60 {
                self.memory_history[i].remove(0);
            }
        }

        // Validate scroll position after data refresh
        // If processes list shrunk, we might need to adjust scroll
        if !self.gpus.is_empty() {
            // For simplicity, we use the first GPU's process count as reference for scrolling
            // In a multi-GPU scenario with independent scrolling, this would need to be per-GPU
            let max_processes = self.gpus[0].processes.len();
            // Assuming visible rows is roughly 10 (this is an approximation, ideally we'd get this from UI layout)
            let visible_rows = 10;

            if max_processes > visible_rows {
                let max_scroll = (max_processes - visible_rows) as u16;
                if self.process_scroll > max_scroll {
                    self.process_scroll = max_scroll;
                }
            } else {
                self.process_scroll = 0;
            }
        }

        Ok(())
    }

    /// Handle keyboard events
    fn handle_events(&mut self) -> anyhow::Result<()> {
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => self.exit = true,
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.process_scroll = self.process_scroll.saturating_sub(1);
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        // Calculate max scroll
                        let max_processes = if !self.gpus.is_empty() {
                            self.gpus[0].processes.len()
                        } else {
                            0
                        };

                        // Approximate visible rows (this should match UI layout)
                        // In ui.rs, the table constraint is Min(12), so roughly 10-12 rows visible
                        let visible_rows = 10;

                        if max_processes > visible_rows {
                            let max_scroll = (max_processes - visible_rows) as u16;
                            if self.process_scroll < max_scroll {
                                self.process_scroll += 1;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
}
