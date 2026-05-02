use std::fs;
use std::path::PathBuf;
use std::process;

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use uica_core::engine;
use uica_decoder::extract_text_from_object;
use uica_model::Invocation;

/// Rust port of uiCA — throughput predictor for x86 basic blocks.
#[derive(Parser, Debug, PartialEq)]
#[command(
    name = "uica-cli",
    version,
    about = "Rust port of uiCA — throughput predictor for x86 basic blocks"
)]
struct Cli {
    /// Input file: object (.o) by default, or a raw byte stream with --raw.
    input: PathBuf,

    /// Treat `input` as a raw byte stream instead of an object file.
    #[arg(long)]
    raw: bool,

    /// Target micro-architecture (e.g. SKL, HSW, ICL).
    #[arg(short = 'a', long)]
    arch: String,

    /// Write JSON result envelope to PATH.
    #[arg(short = 'j', long, value_name = "PATH")]
    json: Option<PathBuf>,

    /// Print only the throughput (cycles per iteration).
    #[arg(long = "tp-only")]
    tp_only: bool,

    /// Write plain-text event trace to PATH for parity debugging.
    #[arg(long = "event-trace", value_name = "PATH")]
    event_trace: Option<PathBuf>,

    /// Write Python-compatible HTML execution trace to PATH.
    #[arg(long = "trace", value_name = "PATH", num_args = 0..=1, default_missing_value = "trace.html")]
    trace: Option<PathBuf>,

    /// Write Python-compatible HTML cumulative graph to PATH.
    #[arg(long = "graph", value_name = "PATH", num_args = 0..=1, default_missing_value = "graph.html")]
    graph: Option<PathBuf>,

    /// Alignment offset within a 64-byte cache line (0..63).
    #[arg(long = "alignment-offset", default_value_t = 0)]
    alignment_offset: u32,

    /// Initial register state policy: diff | same | stack.
    #[arg(long = "init-policy", default_value = "diff")]
    init_policy: String,

    /// Simulate at least this many iterations.
    #[arg(long = "min-iterations", default_value_t = 10)]
    min_iterations: u32,

    /// Simulate at least this many cycles.
    #[arg(long = "min-cycles", default_value_t = 500)]
    min_cycles: u32,

    /// Variant that does not support micro-fusion.
    #[arg(long = "no-micro-fusion")]
    no_micro_fusion: bool,

    /// Variant that does not support macro-fusion.
    #[arg(long = "no-macro-fusion")]
    no_macro_fusion: bool,

    /// Simulate a simple front-end limited only by issue width.
    #[arg(long = "simple-front-end")]
    simple_front_end: bool,
}

impl Cli {
    fn invocation(&self) -> Invocation {
        Invocation {
            arch: self.arch.clone(),
            alignment_offset: self.alignment_offset,
            init_policy: self.init_policy.clone(),
            min_iterations: self.min_iterations,
            min_cycles: self.min_cycles,
            no_micro_fusion: self.no_micro_fusion,
            no_macro_fusion: self.no_macro_fusion,
            simple_front_end: self.simple_front_end,
        }
    }
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{err:#}");
        process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = Cli::parse();
    let invocation = args.invocation();

    let bytes = if args.raw {
        fs::read(&args.input)
            .with_context(|| format!("failed to read raw input {}", args.input.display()))?
    } else {
        extract_text_from_object(&args.input)?
    };

    if let Some(path) = &args.event_trace {
        let trace = uica_core::engine::engine_trace(&bytes, &invocation)
            .map_err(|e| anyhow!("trace engine failed: {e}"))?;
        trace
            .finish_to_path(path)
            .with_context(|| format!("failed to write event trace {}", path.display()))?;
    }

    let wants_reports = args.trace.is_some() || args.graph.is_some();
    let output = if wants_reports {
        uica_core::engine::engine_output(&bytes, &invocation, true)
            .map_err(|e| anyhow!("report engine failed: {e}"))?
    } else {
        uica_core::engine::EngineOutput {
            result: engine(&bytes, &invocation),
            reports: None,
        }
    };
    let result = output.result;

    if let Some(path) = &args.trace {
        let reports = output
            .reports
            .as_ref()
            .ok_or_else(|| anyhow!("report engine did not produce trace data"))?;
        let html = uica_core::report::render_trace_html(&reports.trace)
            .map_err(|e| anyhow!("trace report render failed: {e}"))?;
        fs::write(path, html)
            .with_context(|| format!("failed to write trace report {}", path.display()))?;
    }

    if let Some(path) = &args.graph {
        let reports = output
            .reports
            .as_ref()
            .ok_or_else(|| anyhow!("report engine did not produce graph data"))?;
        let html = uica_core::report::render_graph_html(&reports.graph)
            .map_err(|e| anyhow!("graph report render failed: {e}"))?;
        fs::write(path, html)
            .with_context(|| format!("failed to write graph report {}", path.display()))?;
    }

    if let Some(path) = &args.json {
        let json = serde_json::to_vec_pretty(&result)?;
        fs::write(path, json)
            .with_context(|| format!("failed to write json output {}", path.display()))?;
    }

    if args.tp_only {
        let throughput = result
            .summary
            .throughput_cycles_per_iteration
            .ok_or_else(|| anyhow!("engine did not produce throughput"))?;
        println!("{}", format_throughput(throughput));
    }

    Ok(())
}

fn format_throughput(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{format_throughput, Cli};
    use clap::Parser;
    use std::path::PathBuf;

    #[test]
    fn parses_full_flag_set() {
        let cli = Cli::try_parse_from([
            "uica-cli",
            "loop.bin",
            "--raw",
            "--arch",
            "SKL",
            "--json",
            "out.json",
            "--tp-only",
            "--alignment-offset",
            "4",
            "--init-policy",
            "same",
            "--min-iterations",
            "20",
            "--min-cycles",
            "600",
            "--no-micro-fusion",
            "--no-macro-fusion",
            "--simple-front-end",
            "--event-trace",
            "events.trace",
            "--trace",
            "trace.html",
            "--graph",
            "graph.html",
        ])
        .expect("args should parse");

        assert_eq!(cli.input, PathBuf::from("loop.bin"));
        assert!(cli.raw);
        assert_eq!(cli.arch, "SKL");
        assert_eq!(cli.json, Some(PathBuf::from("out.json")));
        assert!(cli.tp_only);
        assert_eq!(cli.alignment_offset, 4);
        assert_eq!(cli.init_policy, "same");
        assert_eq!(cli.min_iterations, 20);
        assert_eq!(cli.min_cycles, 600);
        assert!(cli.no_micro_fusion);
        assert!(cli.no_macro_fusion);
        assert!(cli.simple_front_end);
        assert_eq!(cli.event_trace, Some(PathBuf::from("events.trace")));
        assert_eq!(cli.trace, Some(PathBuf::from("trace.html")));
        assert_eq!(cli.graph, Some(PathBuf::from("graph.html")));
    }

    #[test]
    fn html_report_flags_default_paths_when_value_omitted() {
        let cli = Cli::try_parse_from([
            "uica-cli",
            "snippet.o",
            "--arch",
            "SKL",
            "--trace",
            "--graph",
        ])
        .expect("html report flags should parse with default paths");

        assert_eq!(cli.trace, Some(PathBuf::from("trace.html")));
        assert_eq!(cli.graph, Some(PathBuf::from("graph.html")));
    }

    #[test]
    fn short_flags_work() {
        let cli = Cli::try_parse_from(["uica-cli", "snippet.o", "-a", "HSW", "-j", "out.json"])
            .expect("short flags should parse");
        assert_eq!(cli.arch, "HSW");
        assert_eq!(cli.json, Some(PathBuf::from("out.json")));
    }

    #[test]
    fn defaults_match_previous_behaviour() {
        let cli = Cli::try_parse_from(["uica-cli", "snippet.o", "--arch", "SKL"])
            .expect("minimal invocation should parse");
        assert_eq!(cli.alignment_offset, 0);
        assert_eq!(cli.init_policy, "diff");
        assert_eq!(cli.min_iterations, 10);
        assert_eq!(cli.min_cycles, 500);
        assert!(!cli.raw);
        assert!(!cli.tp_only);
        assert!(!cli.no_micro_fusion);
        assert!(!cli.no_macro_fusion);
        assert!(!cli.simple_front_end);
        assert_eq!(cli.event_trace, None);
        assert_eq!(cli.trace, None);
        assert_eq!(cli.graph, None);
    }

    #[test]
    fn formats_integer_throughput_without_decimal() {
        assert_eq!(format_throughput(1.0), "1");
        assert_eq!(format_throughput(1.25), "1.25");
    }
}
