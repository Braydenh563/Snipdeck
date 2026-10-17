//! S3 spike: long text + 10k-row list + fuzzy search benchmarks for Slint.
//!
//! `cargo run --release` runs every scenario in its own child process (clean RSS, one Slint
//! platform per process, per-scenario timeout) and prints Markdown tables.
//! `cargo run --release -- --only editor:1mb` runs the scenarios whose id starts with a prefix.
//! `cargo run --release -- --scenario <id>` runs one scenario in-process (used by the parent).

mod editor;
mod gen;
mod harness;
mod list;
mod search;
mod viewer;

slint::include_modules!();

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const SIZES: &[&str] = &["100kb", "1mb", "5mb"];
const SHAPES: &[&str] = &["short", "long", "single"];

fn all_scenarios() -> Vec<String> {
    let mut v = vec!["idle".to_string(), "search".to_string()];
    for kind in ["editor", "viewer"] {
        for size in SIZES {
            for shape in SHAPES {
                for wrap in ["wrap", "nowrap"] {
                    v.push(format!("{kind}:{size}:{shape}:{wrap}"));
                }
            }
        }
    }
    // threshold sweep for the stock TextEdit: where does a keystroke cross one 60 Hz frame?
    for size in ["2kb", "4kb", "8kb", "16kb", "32kb", "64kb"] {
        for shape in ["short", "long"] {
            v.push(format!("editor:{size}:{shape}:wrap"));
        }
    }
    for l in ["listview", "culled", "flickall"] {
        v.push(format!("list:{l}"));
    }
    v
}

fn run_scenario(id: &str) {
    let parts: Vec<&str> = id.split(':').collect();
    match parts.as_slice() {
        ["idle"] => editor::idle(),
        ["search"] => search::run(10_000),
        [kind @ ("editor" | "viewer"), size, shape, wrap] => {
            let label = format!("{size} {shape} {wrap}");
            let sz = gen::parse_size(size);
            let sh = gen::Shape::parse(shape);
            let wr = *wrap == "wrap";
            if *kind == "editor" {
                editor::run(sz, sh, wr, &label)
            } else {
                viewer::run(sz, sh, wr, &label)
            }
        }
        ["list", variant] => list::run(variant, 10_000),
        _ => panic!("unknown scenario {id}"),
    }
}

/// Columns shown per group, in order.
fn columns(group: &str) -> &'static [&'static str] {
    match group {
        "editor" | "viewer" => &[
            "set",
            "layout",
            "first_frame",
            "load_total",
            "repaint",
            "scroll_med",
            "scroll_max",
            "jump_mid",
            "scroll_mid_med",
            "click",
            "keystroke",
            "key_dispatch",
            "key_frame",
            "arrow_down",
            "select_all",
            "scroll_sel_med",
            "shift_down",
            "find_scan",
            "find_select",
            "replace_all",
            "rss_ui",
            "rss_loaded",
            "hwm",
            "render_panics",
        ],
        "list" => &[
            "set_model",
            "first_frame",
            "scroll_med",
            "scroll_p95",
            "scroll_max",
            "jump_mid",
            "jump_end",
            "repaint",
            "filter_match_med",
            "filter_model_med",
            "filter_frame_med",
            "filter_total_med",
            "filter_total_max",
            "rss_data",
            "rss_loaded",
            "hwm",
        ],
        "search" => &[
            "hits",
            "cached_med",
            "cached_p95",
            "cached_max",
            "onthefly_med",
            "onthefly_max",
            "par4_med",
            "par4_max",
            "trunc_hits",
            "trunc_med",
            "trunc_max",
            "trunc_par4_med",
            "trunc_par4_max",
            "inject_10k",
            "per_key_med",
            "per_key_max",
            "threads",
            "utf32_convert",
            "utf32_rss_delta_mb",
        ],
        _ => &["first_frame", "rss_base", "rss_ui", "hwm"],
    }
}

type Row = (String, BTreeMap<String, String>);

fn print_tables(results: &[(String, Row)]) {
    for group in ["idle", "search", "editor", "viewer", "list"] {
        let rows: Vec<&Row> = results
            .iter()
            .filter(|(g, _)| g == group)
            .map(|(_, r)| r)
            .collect();
        if rows.is_empty() {
            continue;
        }
        let cols: Vec<&str> = columns(group)
            .iter()
            .copied()
            .filter(|c| rows.iter().any(|(_, m)| m.contains_key(*c)))
            .collect();
        println!("\n### {group}\n");
        println!("| scenario | {} |", cols.join(" | "));
        println!("|---|{}", "---:|".repeat(cols.len()));
        for (label, m) in &rows {
            let vals: Vec<String> = cols
                .iter()
                .map(|c| m.get(*c).cloned().unwrap_or_else(|| "-".into()))
                .collect();
            println!("| {label} | {} |", vals.join(" | "));
        }
        let notes: Vec<String> = rows
            .iter()
            .filter_map(|(l, m)| {
                m.get("panic_after")
                    .map(|p| format!("- {l}: PANIC after `{p}`"))
            })
            .collect();
        if !notes.is_empty() {
            println!("\n{}", notes.join("\n"));
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--scenario") {
        harness::install_panic_report();
        run_scenario(&args[i + 1]);
        return;
    }
    let only: Vec<String> = args
        .iter()
        .enumerate()
        .filter(|(i, _)| *i > 0 && args[i - 1] == "--only")
        .map(|(_, a)| a.clone())
        .collect();
    let timeout = args
        .iter()
        .position(|a| a == "--timeout")
        .map(|i| args[i + 1].parse::<u64>().unwrap())
        .unwrap_or(300);
    let exe = std::env::current_exe().unwrap();
    let mut results: Vec<(String, Row)> = Vec::new();
    eprintln!(
        "S3 long-text benchmark: software renderer, {}x{} editor / {}x{} list, scale 1.0, timeout {timeout}s per scenario",
        editor::W,
        editor::H,
        list::W,
        list::H
    );
    for id in all_scenarios() {
        if !only.is_empty() && !only.iter().any(|p| id.starts_with(p.as_str())) {
            continue;
        }
        let start = Instant::now();
        let mut child = Command::new(&exe)
            .args(["--scenario", &id])
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .env("RUST_BACKTRACE", "0")
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let reader = std::thread::spawn(move || {
            BufReader::new(stdout)
                .lines()
                .map_while(Result::ok)
                .collect::<Vec<_>>()
        });
        let status = loop {
            if let Some(s) = child.try_wait().unwrap() {
                break Some(s);
            }
            if start.elapsed() > Duration::from_secs(timeout) {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let lines = reader.join().unwrap();
        let mut got = false;
        for l in lines {
            let mut it = l.split('\t');
            if it.next() != Some("RESULT") {
                continue;
            }
            let group = it.next().unwrap().to_string();
            let label = it.next().unwrap().to_string();
            let m: BTreeMap<String, String> = it
                .filter_map(|kv| kv.split_once('='))
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            results.push((group, (label, m)));
            got = true;
        }
        let secs = start.elapsed().as_secs_f64();
        match status {
            None => {
                eprintln!("  {id}: TIMEOUT after {secs:.0}s");
                let group = id.split(':').next().unwrap().to_string();
                let label = id.split_once(':').map_or("", |x| x.1).replace(':', " ");
                let mut m = BTreeMap::new();
                m.insert("set".into(), format!("TIMEOUT >{timeout}s"));
                results.push((group, (label, m)));
            }
            Some(s) if !s.success() || !got => eprintln!("  {id}: FAILED ({s}) after {secs:.1}s"),
            Some(_) => eprintln!("  {id}: done in {secs:.1}s"),
        }
    }
    print_tables(&results);
}
