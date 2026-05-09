use std::fs;
use std::path::PathBuf;
use std::process;

use anyhow::{anyhow, Context, Result};
use clap::Parser;
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
    #[arg(required_unless_present = "arch_list")]
    input: Option<PathBuf>,

    /// List real micro-architectures supported by the simulator and exit.
    #[arg(long = "arch-list", exclusive = true)]
    arch_list: bool,

    /// Treat `input` as a raw byte stream instead of an object file.
    #[arg(long)]
    raw: bool,

    /// Target micro-architecture (e.g. SKL, HSW, ICL).
    #[arg(short = 'a', long, required_unless_present = "arch_list")]
    arch: Option<String>,

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

    /// Verify UIPack checksum on first datapack load.
    #[arg(long = "verify-uipack")]
    verify_uipack: bool,

    /// Simulate a simple front-end limited only by issue width.
    #[arg(long = "simple-front-end")]
    simple_front_end: bool,
}

impl Cli {
    fn invocation(&self) -> Invocation {
        Invocation {
            arch: self.arch.clone().unwrap_or_default(),
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
    if args.arch_list {
        for arch in uica_core::supported_real_arches() {
            println!("{arch}");
        }
        return Ok(());
    }

    let input = args
        .input
        .as_ref()
        .ok_or_else(|| anyhow!("missing input path"))?;
    let invocation = args.invocation();

    let bytes = if args.raw {
        fs::read(input).with_context(|| format!("failed to read raw input {}", input.display()))?
    } else {
        extract_text_from_object(input)?
    };

    if let Some(path) = &args.event_trace {
        let trace = uica_core::engine::engine_trace(&bytes, &invocation, args.verify_uipack)
            .map_err(|e| anyhow!("trace engine failed: {e}"))?;
        trace
            .finish_to_path(path)
            .with_context(|| format!("failed to write event trace {}", path.display()))?;
    }

    let wants_default_text = wants_default_text(&args);
    let wants_reports = args.trace.is_some() || args.graph.is_some() || wants_default_text;
    let output = if wants_reports {
        uica_core::engine::engine_output(&bytes, &invocation, true, args.verify_uipack)
            .map_err(|e| anyhow!("report engine failed: {e}"))?
    } else {
        uica_core::engine::EngineOutput {
            result: if args.verify_uipack {
                uica_core::engine::engine_output(&bytes, &invocation, false, true)
                    .map_err(|e| anyhow!("uipack verification failed: {e}"))?
                    .result
            } else {
                uica_core::engine::engine_output(&bytes, &invocation, false, false)
                    .map_err(|e| anyhow!("engine failed: {e}"))?
                    .result
            },
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

    if wants_default_text {
        let reports = output
            .reports
            .as_ref()
            .ok_or_else(|| anyhow!("report engine did not produce regular text data"))?;
        print!(
            "{}",
            uica_core::report::render_regular_text(&reports.regular)
        );
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

fn wants_default_text(args: &Cli) -> bool {
    !args.tp_only
        && args.json.is_none()
        && args.trace.is_none()
        && args.graph.is_none()
        && args.event_trace.is_none()
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
    use super::{format_throughput, wants_default_text, Cli};
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
            "--verify-uipack",
            "--event-trace",
            "events.trace",
            "--trace",
            "trace.html",
            "--graph",
            "graph.html",
        ])
        .expect("args should parse");

        assert_eq!(cli.input, Some(PathBuf::from("loop.bin")));
        assert!(cli.raw);
        assert_eq!(cli.arch.as_deref(), Some("SKL"));
        assert_eq!(cli.json, Some(PathBuf::from("out.json")));
        assert!(cli.tp_only);
        assert_eq!(cli.alignment_offset, 4);
        assert_eq!(cli.init_policy, "same");
        assert_eq!(cli.min_iterations, 20);
        assert_eq!(cli.min_cycles, 600);
        assert!(cli.no_micro_fusion);
        assert!(cli.no_macro_fusion);
        assert!(cli.simple_front_end);
        assert!(cli.verify_uipack);
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
        assert_eq!(cli.arch.as_deref(), Some("HSW"));
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
        assert!(!cli.verify_uipack);
        assert_eq!(cli.event_trace, None);
        assert_eq!(cli.trace, None);
        assert_eq!(cli.graph, None);
    }

    #[test]
    fn arch_list_parses_without_input_or_arch() {
        let cli = Cli::try_parse_from(["uica-cli", "--arch-list"])
            .expect("arch list should parse by itself");
        assert!(cli.arch_list);
        assert_eq!(cli.input, None);
    }

    #[test]
    fn arch_list_rejects_arch() {
        let err = Cli::try_parse_from(["uica-cli", "--arch-list", "--arch", "SKL"])
            .expect_err("arch list should conflict with arch");
        assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
    }

    #[test]
    fn arch_list_rejects_other_flags_and_input() {
        let raw_err = Cli::try_parse_from(["uica-cli", "--arch-list", "--raw"])
            .expect_err("arch list should conflict with raw");
        assert_eq!(raw_err.kind(), clap::error::ErrorKind::ArgumentConflict);

        let input_err = Cli::try_parse_from(["uica-cli", "loop.bin", "--arch-list"])
            .expect_err("arch list should conflict with input");
        assert_eq!(input_err.kind(), clap::error::ErrorKind::ArgumentConflict);
    }

    #[test]
    fn default_text_is_suppressed_by_output_modes() {
        let default_cli = Cli::try_parse_from(["uica-cli", "snippet.o", "--arch", "SKL"])
            .expect("minimal invocation should parse");
        assert!(wants_default_text(&default_cli));

        for args in [
            vec![
                "uica-cli",
                "snippet.o",
                "--arch",
                "SKL",
                "--json",
                "out.json",
            ],
            vec![
                "uica-cli",
                "snippet.o",
                "--arch",
                "SKL",
                "--trace",
                "trace.html",
            ],
            vec![
                "uica-cli",
                "snippet.o",
                "--arch",
                "SKL",
                "--graph",
                "graph.html",
            ],
            vec![
                "uica-cli",
                "snippet.o",
                "--arch",
                "SKL",
                "--event-trace",
                "events.trace",
            ],
            vec!["uica-cli", "snippet.o", "--arch", "SKL", "--tp-only"],
        ] {
            let cli = Cli::try_parse_from(args).expect("output mode invocation should parse");
            assert!(!wants_default_text(&cli));
        }
    }

    #[test]
    fn formats_integer_throughput_without_decimal() {
        assert_eq!(format_throughput(1.0), "1");
        assert_eq!(format_throughput(1.25), "1.25");
    }
}
