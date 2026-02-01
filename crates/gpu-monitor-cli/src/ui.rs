//! UI rendering for TUI

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Paragraph, Row, Sparkline, Table,
    },
    Frame,
};

use crate::app::App;

/// Main draw function
pub fn draw(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(10),   // GPU cards
            Constraint::Length(1), // Footer
        ])
        .split(frame.area());

    // Header
    draw_header(frame, chunks[0]);

    // GPU cards (one per GPU)
    if !app.gpus.is_empty() {
        let gpu_constraints: Vec<Constraint> = app
            .gpus
            .iter()
            .map(|_| Constraint::Min(12)) // Compact height
            .collect();

        let gpu_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(gpu_constraints)
            .split(chunks[1]);

        for (i, gpu) in app.gpus.iter().enumerate() {
            if i < gpu_chunks.len() {
                let history = app.gpu_history.get(i).map(|h| h.as_slice()).unwrap_or(&[]);
                let mem_history = app.memory_history.get(i).map(|h| h.as_slice()).unwrap_or(&[]);
                draw_gpu_card(frame, gpu_chunks[i], gpu, history, mem_history, app.process_scroll);
            }
        }
    } else {
        let msg = Paragraph::new("No GPU data available. Make sure NVIDIA drivers are installed.")
            .style(Style::default().fg(Color::Yellow))
            .block(Block::default().borders(Borders::ALL).title("GPU Monitor"));
        frame.render_widget(msg, chunks[1]);
    }

    // Footer
    draw_footer(frame, chunks[2]);
}

/// Draw header
fn draw_header(frame: &mut Frame, area: Rect) {
    let header = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(Span::styled(
            " GPU Monitor ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));

    let inner = header.inner(area);
    frame.render_widget(header, area);

    let text = Paragraph::new(Line::from(vec![
        Span::styled("Real-time GPU monitoring", Style::default().fg(Color::White)),
        Span::raw(" │ "),
        Span::styled("Press ", Style::default().fg(Color::DarkGray)),
        Span::styled("q", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::styled(" to quit", Style::default().fg(Color::DarkGray)),
    ]));
    frame.render_widget(text, inner);
}

/// Draw footer
fn draw_footer(frame: &mut Frame, area: Rect) {
    let footer = Paragraph::new(Line::from(vec![
        Span::styled("↑/↓", Style::default().fg(Color::Yellow)),
        Span::raw(" scroll │ "),
        Span::styled("q", Style::default().fg(Color::Yellow)),
        Span::raw(" quit"),
    ]))
    .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(footer, area);
}

/// Draw a single GPU card
fn draw_gpu_card(
    frame: &mut Frame,
    area: Rect,
    gpu: &gpu_monitor_core::GpuInfo,
    gpu_history: &[u64],
    mem_history: &[u64],
    process_scroll: u16,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue))
        .title(Span::styled(
            format!(" GPU {}: {} ", gpu.device.index, gpu.device.name),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Split into left (metrics) and right (processes)
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
        .split(inner);

    // Left side: metrics
    draw_metrics(frame, chunks[0], gpu, gpu_history, mem_history);

    // Right side: processes
    draw_processes(frame, chunks[1], &gpu.processes, process_scroll);
}

/// Draw GPU metrics
fn draw_metrics(
    frame: &mut Frame,
    area: Rect,
    gpu: &gpu_monitor_core::GpuInfo,
    gpu_history: &[u64],
    mem_history: &[u64],
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Info row
            Constraint::Length(1), // Spacer
            Constraint::Length(3), // GPU Chart
            Constraint::Length(1), // Spacer
            Constraint::Length(3), // Memory Chart
        ])
        .split(area);

    // Info Row
    let temp_color = match gpu.metrics.temperature_status() {
        gpu_monitor_core::metrics::TemperatureStatus::Cool => Color::Green,
        gpu_monitor_core::metrics::TemperatureStatus::Normal => Color::Blue,
        gpu_monitor_core::metrics::TemperatureStatus::Warm => Color::Yellow,
        gpu_monitor_core::metrics::TemperatureStatus::Hot => Color::Red,
    };

    let info_text = Line::from(vec![
        Span::raw("Temp: "),
        Span::styled(format!("{}°C", gpu.metrics.temperature), Style::default().fg(temp_color)),
        Span::raw("  Power: "),
        Span::styled(format!("{:.0}W", gpu.metrics.power_watts()), Style::default().fg(Color::Yellow)),
        Span::raw("  Fan: "),
        Span::styled(
            format!("{}%", gpu.metrics.fan_speed.map(|f| f.to_string()).unwrap_or_else(|| "N/A".to_string())),
            Style::default().fg(Color::Cyan)
        ),
        Span::raw("  Clock: "),
        Span::styled(format!("{}MHz", gpu.metrics.clock_graphics), Style::default().fg(Color::Magenta)),
    ]);
    frame.render_widget(Paragraph::new(info_text), chunks[0]);

    // GPU Chart Section
    let gpu_color = if gpu.metrics.gpu_utilization > 80 {
        Color::Red
    } else if gpu.metrics.gpu_utilization > 50 {
        Color::Yellow
    } else {
        Color::Green
    };

    // Title with real-time value
    let gpu_title = format!("GPU Load: {}%", gpu.metrics.gpu_utilization);

    let gpu_sparkline = Sparkline::default()
        .block(Block::default().title(gpu_title).borders(Borders::NONE))
        .data(gpu_history)
        .max(100)
        .style(Style::default().fg(gpu_color));
    frame.render_widget(gpu_sparkline, chunks[2]);

    // Memory Chart Section
    let mem_percent = gpu.memory.usage_percent() as u16;
    let mem_color = if mem_percent > 80 {
        Color::Red
    } else if mem_percent > 50 {
        Color::Yellow
    } else {
        Color::Cyan
    };

    // Title with real-time value
    let mem_title = format!(
        "Memory: {:.1} / {:.1} GiB ({:.0}%)",
        gpu.memory.used_gib(),
        gpu.memory.total_gib(),
        gpu.memory.usage_percent()
    );

    let mem_sparkline = Sparkline::default()
        .block(Block::default().title(mem_title).borders(Borders::NONE))
        .data(mem_history)
        .max(100)
        .style(Style::default().fg(mem_color));
    frame.render_widget(mem_sparkline, chunks[4]);
}

/// Draw GPU processes
fn draw_processes(
    frame: &mut Frame,
    area: Rect,
    processes: &[gpu_monitor_core::GpuProcess],
    scroll: u16,
) {
    let header = Row::new(vec!["PID", "Name", "Mem", "Type"])
        .style(Style::default().add_modifier(Modifier::BOLD).fg(Color::Cyan));

    let rows: Vec<Row> = processes
        .iter()
        .skip(scroll as usize)
        .map(|p| {
            Row::new(vec![
                p.pid.to_string(),
                truncate_str(&p.name, 15),
                format!("{}M", p.gpu_memory_mib()),
                p.process_type.short_label().to_string(),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(7),
            Constraint::Min(10),
            Constraint::Length(8),
            Constraint::Length(6),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::LEFT)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(format!("Processes ({})", processes.len())),
    )
    .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    frame.render_widget(table, area);
}

/// Truncate string to max length
fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}
