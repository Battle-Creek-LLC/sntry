use anyhow::Result;
use clap::ValueEnum;
use comfy_table::Table;
use is_terminal::IsTerminal;
use serde_json::Value;
use std::io::Write;

#[derive(Copy, Clone, Debug, ValueEnum, PartialEq, Eq)]
#[value(rename_all = "lowercase")]
pub enum OutputFormat {
    Text,
    Json,
    Ndjson,
    Table,
}

pub fn resolve_output(cli: Option<OutputFormat>, default_from_config: Option<&str>) -> OutputFormat {
    if let Some(o) = cli {
        return o;
    }
    if let Ok(env) = std::env::var("SNTRY_OUTPUT") {
        if let Some(o) = parse_output(&env) {
            return o;
        }
    }
    if let Some(s) = default_from_config {
        if let Some(o) = parse_output(s) {
            return o;
        }
    }
    if std::io::stdout().is_terminal() {
        OutputFormat::Text
    } else {
        OutputFormat::Json
    }
}

fn parse_output(s: &str) -> Option<OutputFormat> {
    match s.to_ascii_lowercase().as_str() {
        "text" => Some(OutputFormat::Text),
        "json" => Some(OutputFormat::Json),
        "ndjson" => Some(OutputFormat::Ndjson),
        "table" => Some(OutputFormat::Table),
        _ => None,
    }
}

pub fn print_value(format: OutputFormat, value: &Value, columns: &[Column]) -> Result<()> {
    match format {
        OutputFormat::Json => print_json_pretty(value),
        OutputFormat::Ndjson => print_ndjson(value),
        OutputFormat::Text => print_text(value, columns),
        OutputFormat::Table => print_table(value, columns),
    }
}

pub fn print_empty(format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Json => {
            println!("[]");
        }
        OutputFormat::Text | OutputFormat::Table => {
            eprintln!("No results.");
        }
        OutputFormat::Ndjson => {}
    }
    Ok(())
}

fn print_json_pretty(value: &Value) -> Result<()> {
    if std::io::stdout().is_terminal() {
        serde_json::to_writer_pretty(std::io::stdout().lock(), value)?;
    } else {
        serde_json::to_writer(std::io::stdout().lock(), value)?;
    }
    println!();
    Ok(())
}

fn print_ndjson(value: &Value) -> Result<()> {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    match value {
        Value::Array(arr) => {
            for item in arr {
                serde_json::to_writer(&mut out, item)?;
                writeln!(out)?;
            }
        }
        other => {
            serde_json::to_writer(&mut out, other)?;
            writeln!(out)?;
        }
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct Column {
    pub header: &'static str,
    pub path: &'static [&'static str],
}

impl Column {
    pub const fn new(header: &'static str, path: &'static [&'static str]) -> Self {
        Self { header, path }
    }
}

fn extract(value: &Value, path: &[&str]) -> String {
    let mut current = value;
    for key in path {
        current = match current.get(*key) {
            Some(v) => v,
            None => return String::new(),
        };
    }
    match current {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

fn print_text(value: &Value, columns: &[Column]) -> Result<()> {
    let rows: Vec<&Value> = match value {
        Value::Array(arr) => arr.iter().collect(),
        Value::Object(_) => vec![value],
        _ => return Ok(()),
    };
    if columns.is_empty() {
        return print_json_pretty(value);
    }

    // Compute column widths.
    let mut widths: Vec<usize> = columns.iter().map(|c| c.header.len()).collect();
    let cells: Vec<Vec<String>> = rows
        .iter()
        .map(|row| {
            columns
                .iter()
                .map(|c| extract(row, c.path))
                .collect::<Vec<_>>()
        })
        .collect();
    for row in &cells {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.chars().count().min(120));
        }
    }

    // Header.
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for (i, col) in columns.iter().enumerate() {
        if i > 0 {
            write!(out, "  ")?;
        }
        write!(out, "{:<width$}", col.header, width = widths[i])?;
    }
    writeln!(out)?;
    for row in &cells {
        for (i, cell) in row.iter().enumerate() {
            if i > 0 {
                write!(out, "  ")?;
            }
            let truncated: String = cell.chars().take(120).collect();
            write!(out, "{:<width$}", truncated, width = widths[i])?;
        }
        writeln!(out)?;
    }
    Ok(())
}

fn print_table(value: &Value, columns: &[Column]) -> Result<()> {
    let rows: Vec<&Value> = match value {
        Value::Array(arr) => arr.iter().collect(),
        Value::Object(_) => vec![value],
        _ => return Ok(()),
    };
    if columns.is_empty() {
        return print_json_pretty(value);
    }
    let mut table = Table::new();
    table.set_header(columns.iter().map(|c| c.header));
    for row in rows {
        table.add_row(columns.iter().map(|c| extract(row, c.path)));
    }
    println!("{}", table);
    Ok(())
}
