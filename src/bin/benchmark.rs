
use std::{io::{BufRead, BufReader, BufWriter, Write}, path, process::{ChildStderr, ChildStdin, ChildStdout, Command, ExitCode, Stdio}, time::{Duration, SystemTime}};
use clap::Parser;

static SYNTHETIC_LOAD_NAME: &str = "synthetic_load";

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Path to scanmem program to run.
    #[arg(long)]
    scanmem_program: String,

    /// Benchmark to run, use --list_benchmarks to list available benchmarks.
    #[arg(short = 'b', long)]
    benchmark: Option<String>,

    /// Number of threads scanmem will use to scan, set to -1 if multi threading is not supported by the scanmem program. 
    #[arg(short = 't', long, default_value_t = -1)]
    nthreads: i32,

    /// Minimum size of synthetic load at start (in bytes).
    #[arg(long, default_value_t = 0x1_000_000u64)]
    minbytes: u64,
    /// Maximum size of synthetic load at end (in bytes).
    #[arg(long, default_value_t = 0x1_000_000u64)]
    maxbytes: u64,
    /// Fixed increment added to size between each run (in bytes).
    #[arg(long, default_value_t = 0x1_000_000u64)]
    stepbytes: u64,
    /// Multiplication factor applied to size between each run (applied after stepbytes) (in bytes) (floating point). 
    #[arg(long, default_value_t = 1.0f64)]
    stepfactor: f64,

    /// Number of iterations per scenario.
    #[arg(short = 'n', long, default_value_t = 20)]
    iterations: usize,

    /// Timeout test if time elapsed is longer than specified (in seconds), 0 disables timeout.
    #[arg(short = 'T', long, default_value_t = 0)]
    timeout: u64,

    #[arg(long, default_value_t = 0x1u64)]
    synthetic_load_random_seed: u64,

    /// List available benchmarks amd exit.
    #[arg(short = 'l', long, default_value_t = false)]
    list_benchmarks: bool,

    /// csv output
    #[arg[long]]
    csv_output: Option<path::PathBuf>,

    /// Echo child process stdout and stderr in parent stdout and stderr.
    #[arg(short = 'v', long, default_value_t = false)]
    verbose: bool,
}

#[derive(Default, Debug)]
struct BenchmarkIteration {
    benchmark_time: Duration,
    end_matches: u64, // Number of matches found at end of iteration
}

#[derive(Default, Debug)]
struct BenchmarkResult {
    // params
    synthetic_load_size: u64, 
    synthetic_load_random_seed: u64,
    
    setup_time: Duration,
    total_time: Duration,

    iterations: Vec<BenchmarkIteration>,

    // aggregates (in seconds)
    mean: f64,
    median: f64,
    min: f64,
    max: f64,
    standard_deviation: f64,

}

#[derive(Default, Debug)]
struct BenckmarkReport {
    // metadata
    scanmem_program: String,
    benchmark_name: String,
    nthreads: i32,
    minbytes: u64,
    maxbytes: u64,
    stepbytes: u64,
    stepfactor: f64,
    iteration_count: usize,
    timeout: u64,

    // results
    results: Vec<BenchmarkResult>,
}

fn create_csv_row(elements: &Vec<&str>) -> String {
    let mut ret = String::new();
    for e in elements {
        ret.push_str(e);
        ret.push(',');
    }
    return ret;
}

fn benchmark_report_to_csv(report: &BenckmarkReport) -> String {
    let mut ret = String::new();

    let scanmem_program = &report.scanmem_program;
    let benchmark_name = &report.benchmark_name;
    let nthreads = report.nthreads.to_string();


    // Create header.
    ret.push_str(&create_csv_row(&vec!["scanmem_program", "benchmark_name", "nthreads", "synthetic_load_size(bytes)", "synthetic_load_random_seed", "setup_time(ms)", "total_time(ms)", "iteration", "benchmark_time(ms)"]));
    ret.push('\n');
    for result in &report.results {
        let synthetic_load_size = result.synthetic_load_size.to_string();
        let synthetic_load_random_seed = result.synthetic_load_random_seed.to_string();
        let setup_time = result.setup_time.as_millis().to_string();
        let total_time = result.total_time.as_millis().to_string();
        for iteration in 0..result.iterations.len() {
            let benchmark_time = result.iterations[iteration].benchmark_time.as_millis().to_string();
            ret.push_str(&create_csv_row(&vec![scanmem_program, benchmark_name, &nthreads, &synthetic_load_size, &synthetic_load_random_seed, &setup_time, &total_time, &iteration.to_string(), &benchmark_time]));
            ret.push('\n');
        }
    }

    return ret;
}

fn read_line_stream<S: BufRead>(stream: &mut S, pid: u32, echo: bool, name: &str) -> Result<String, String> {
    let mut buf = String::new();
    stream.read_line(&mut buf).map_err(|e|e.to_string())?;
    if echo && !buf.is_empty() {
        print!("pid {} {}: {}", pid, name, buf);
    }
    return Ok(buf)
}

/// Read 1 line from stdout and return it, blocking.
fn read_line_stdout(stdout: & mut BufReader<ChildStdout>, pid: u32, echo: bool) -> Result<String, String> {
    return read_line_stream(stdout, pid, echo, "stdout");
}

/// Read 1 line from stderr and return it, blocking.
fn read_line_stderr(stderr: & mut BufReader<ChildStderr>, pid: u32, echo: bool) -> Result<String, String> {
    return read_line_stream(stderr, pid, echo, "stderr");
}

/// Read from stdout until exact line is present.
/// Discards read lines.
fn read_until_line_stdout(stdout: & mut BufReader<ChildStdout>, condition_line: &str, pid: u32, echo: bool) -> Result<(), String> {
    loop {
        let buf = read_line_stdout(stdout, pid, echo)?;
        if buf.eq(format!("{}\n", condition_line).as_str()) {
            return Ok(())
        }
    }
}

// Write line to stream, blocking.
fn write_line(stdin: &mut BufWriter<ChildStdin>, line: &str, pid: u32, echo: bool) -> Result<(), String> {
    let out = format!("{}\n", line);
    if echo {
        print!("pid {} stdin: {}", pid, out);
    }
    stdin.write_all(out.as_bytes()).map_err(|e|e.to_string())?;
    stdin.flush().map_err(|e|e.to_string())?;
    return Ok(())
}

// Read whats left in the output pipe.
fn drain_stream<S: BufRead>(stream: &mut S, pid: u32, echo: bool, name: &str) -> Result<(), String> {
    loop {
        let buf = read_line_stream(stream, pid, echo, name)?;                
        if buf.len() == 0 {
            break;
        }
    }
    Ok(())
}

fn drain_stdout(stdout: & mut BufReader<ChildStdout>, pid: u32, echo: bool) -> Result<(), String> {
    return drain_stream(stdout, pid, echo, "stdout");
}

fn drain_stderr(stderr: & mut BufReader<ChildStderr>, pid: u32, echo: bool) -> Result<(), String> {
    return drain_stream(stderr, pid, echo, "stderr");
}

fn parse_scanmem_commands(input: &str) -> Vec<&str> {

    let ret: Vec<&str> = input.split(';').collect();

    // check if last command is 'exit'
    if let Some(last) = ret.last() {
        if !last.trim_ascii().eq("exit") {
            println!("Warning: scanmem commands does not exit with 'exit'!.");
        }
    }
    return ret;
}

fn compute_median<I>(values: I) -> f64 where I: Iterator<Item = f64>, {
    let mut data: Vec<f64> = values.collect();
    data.sort_by(|a,b|a.total_cmp(b));
    return data[data.len() / 2];
}

fn compute_standard_deviation<I>(values: I, mean: f64) -> f64 where I: Iterator<Item = f64>, {
    let data: Vec<f64> = values.collect();
    let len = data.len();
    let sum = data.into_iter().reduce(|acc: f64, e: f64| acc + (e - mean)).unwrap();
    return f64::sqrt(1.0f64 / len as f64 * sum.powi(2));
}

type BenchmarkScenarioFunc = fn(result: &mut BenchmarkResult, scanmem_program: &str, synthetic_load_program: &str, synthetic_load_size: u64, synthetic_load_random_seed: u64, iteration_count: usize, nthreads: i32, verbose: bool) -> Result<(), String>;

fn scenario_func_fill_random_iteration(scanmem_program: &str, scanmem_commands: &Vec<&str>, target_process_pid: u32, nthreads: i32, verbose: bool) -> Result<(), String> {
    
    // Create scanmem child process
    println!("Starting scanmem child process...");
    let args: String;
    if nthreads == -1 {
        args = format!("--pid={}", target_process_pid);
    }
    else {
        args = format!("--pid={} --jobs={}", target_process_pid, nthreads);
    }

    let args_vec: Vec<&str> = args.split_ascii_whitespace().collect();

    let scanmem_exit_status;

    // I don't like this.
    let error_arc  = std::sync::Arc::<std::sync::atomic::AtomicBool>::new(false.into());
    let error_arc_2  = error_arc.clone();

    {
        let mut scanmem_process = match Command::new(scanmem_program).args(args_vec).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn() {
            Ok(c) => c,
            Err(e) => {
                return Err(e.to_string())    
            }
        };
        let mut stdin = BufWriter::new(scanmem_process.stdin.take().unwrap());
        let mut stdout = BufReader::new(scanmem_process.stdout.take().unwrap());
        let mut stderr = BufReader::new(scanmem_process.stderr.take().unwrap());

        let pid = scanmem_process.id();

        // Spawn threads to drain stdout and stderr.
        let stdout_thread = std::thread::spawn(move || {
            drain_stdout(&mut stdout, pid, verbose).unwrap();
        });

        let stderr_thread = std::thread::spawn(move || {
            let error_arc_2 = error_arc_2;
            loop {
                let buf = read_line_stderr(&mut stderr, pid, verbose).unwrap();
                if buf.len() == 0 {
                    break;
                }
                
                // Check if we have a permission error when running scanmem.
                if buf.contains("Operation not permitted") {
                    error_arc_2.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            }
        });

        for command in scanmem_commands {
            write_line(&mut stdin, command, pid, verbose)?;
        }

        // Drain output pipes.
        stdout_thread.join().unwrap();
        stderr_thread.join().unwrap();
    
        // Cleanup
        scanmem_exit_status = scanmem_process.wait().unwrap();
    }

    if !scanmem_exit_status.success() {
        return Err(format!("Error: scanmem did not exit successfully, ExitStatus = {} ({})", scanmem_exit_status.code().unwrap(), scanmem_exit_status.to_string()));
    }

    let error = error_arc.load(std::sync::atomic::Ordering::Relaxed);

    if error {
        return Err(format!("Error: Interactive error detected during execution of scanmem, look at stderr output for more info"));
    }

    println!("scanmem child process done");
    
    return Ok(())
}

fn scenario_func_fill_random(result: &mut BenchmarkResult, scanmem_program: &str, synthetic_load_program: &str, synthetic_load_size: u64, synthetic_load_random_seed: u64, iteration_count: usize, nthreads: i32, verbose: bool) -> Result<(), String> {

    let scanmem_commands = vec!["= 1", "exit"];

    let iterations = &mut result.iterations;

    let total_start_time = SystemTime::now();

    // Create synthetic_load child process and init.
    println!("Starting synthetic_load child process...");
    
    let mut synthetic_load_process = match Command::new(synthetic_load_program).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn() {
        Ok(c) => c,
        Err(e) => {
            return Err(e.to_string())    
        }
    };
    let mut stdin = BufWriter::new(synthetic_load_process.stdin.take().unwrap());
    let mut stdout = BufReader::new(synthetic_load_process.stdout.take().unwrap());
    let pid = synthetic_load_process.id();

    // Init synthetic_load.
    write_line(&mut stdin, format!("set-memory-size {}", synthetic_load_size).as_str(), pid, verbose)?;
    read_until_line_stdout(&mut stdout, "Done", pid, verbose)?;
    write_line(&mut stdin, format!("fill-random {}", synthetic_load_random_seed).as_str(), pid, verbose)?;
    read_until_line_stdout(&mut stdout, "Done", pid, verbose)?;

    // Run benchmark.
    result.setup_time = SystemTime::now().duration_since(total_start_time).map_err(|e|e.to_string())?;

    iterations.reserve(iteration_count);
    for _ in 0..iteration_count {
        let start = SystemTime::now();
        scenario_func_fill_random_iteration(scanmem_program, &scanmem_commands, pid, nthreads, verbose)?;
        
        let duration = SystemTime::now().duration_since(start).map_err(|e|e.to_string())?;
        iterations.push(BenchmarkIteration{benchmark_time: duration, end_matches: 0});
    }

    // Exit synthetic_load.
    write_line(&mut stdin, format!("exit").as_str(), pid, verbose)?;
    drain_stdout(&mut stdout, pid, verbose)?;

    let synthetic_load_exit_status = synthetic_load_process.wait().unwrap();
    if !synthetic_load_exit_status.success() {
        return Err(format!("Error: synthetic_load did not exit successfully, ExitStatus = {} ({})", synthetic_load_exit_status.code().unwrap(), synthetic_load_exit_status.to_string()));
    }

    println!("synthetic_load child process done");

    result.total_time = SystemTime::now().duration_since(total_start_time).map_err(|e|e.to_string())?;

    return Ok(())
}

struct BenchmarkScenario {
    name: String,
    description: String,
    perform_benchmark_scenario_func: BenchmarkScenarioFunc,
}

fn main() -> ExitCode {

    let cli = Cli::parse();

    let benchmark_list: Vec<BenchmarkScenario> = vec![
        BenchmarkScenario{
            name: "FillRandom".into(),
            description: "Fill target process with random bytes, then call scanmem with \"= 1; 1\".".into(),
            perform_benchmark_scenario_func: scenario_func_fill_random
        },
    ];

    if cli.list_benchmarks {

        for e in benchmark_list {
            println!("{}", e.name);
            println!("\t{}", e.description);
        }

        return ExitCode::SUCCESS;
    }

    // Select benchmark scenario.
    let benchmark_name: String;
    if cli.benchmark.is_none() {
        println!("Error: No benchmark selected.");
        return ExitCode::FAILURE;
    }
    benchmark_name = cli.benchmark.unwrap();

    let benchmark_scenario; 
    match benchmark_list.iter().find(|x|x.name.eq_ignore_ascii_case(&benchmark_name)) {
        Some(x) => benchmark_scenario = x,
        None => {
            println!("Error: Benchmark \"{}\" not found.", benchmark_name);
            return ExitCode::FAILURE;
        }
    }

    let synthetic_load_path = std::env::current_exe().unwrap().parent().unwrap().to_path_buf().join(SYNTHETIC_LOAD_NAME);
    
    let mut report = BenckmarkReport::default();
    report.scanmem_program = cli.scanmem_program;
    report.benchmark_name = benchmark_scenario.name.clone();
    report.nthreads = cli.nthreads;
    report.minbytes = cli.minbytes;
    report.maxbytes = cli.maxbytes;
    report.stepbytes = cli.stepbytes;
    report.stepfactor = cli.stepfactor;
    report.iteration_count = cli.iterations;
    report.timeout = cli.timeout;

    let mut step_size = report.minbytes;
    while step_size >= report.minbytes && step_size <= report.maxbytes {
        
        let mut benchmark_result = BenchmarkResult::default();

        let synthetic_load_size = step_size;
        let synthetic_load_random_seed = cli.synthetic_load_random_seed;

        benchmark_result.synthetic_load_size = synthetic_load_size;
        benchmark_result.synthetic_load_random_seed = synthetic_load_random_seed; 

        match (benchmark_scenario.perform_benchmark_scenario_func)(&mut benchmark_result, &report.scanmem_program, synthetic_load_path.to_str().unwrap(), synthetic_load_size, synthetic_load_random_seed, cli.iterations, report.nthreads, cli.verbose) {
            Ok(()) => {},
            Err(err) => {
                println!("Benchmark failed: {}", err);
                return ExitCode::FAILURE;
            }
        }

        // compute aggregates
        benchmark_result.max = benchmark_result.iterations.iter().map(|e|e.benchmark_time.as_secs_f64()).max_by(|a,b|a.total_cmp(b)).unwrap();
        benchmark_result.min = benchmark_result.iterations.iter().map(|e|e.benchmark_time.as_secs_f64()).min_by(|a,b|a.total_cmp(b)).unwrap();
        benchmark_result.mean = benchmark_result.iterations.iter().map(|e|e.benchmark_time.as_secs_f64()).sum::<f64>() / benchmark_result.iterations.len() as f64;
        benchmark_result.standard_deviation = compute_standard_deviation(benchmark_result.iterations.iter().map(|e|e.benchmark_time.as_secs_f64()), benchmark_result.mean);
        benchmark_result.median = compute_median(benchmark_result.iterations.iter().map(|e|e.benchmark_time.as_secs_f64()));

        report.results.push(benchmark_result);

        // next step
        step_size += report.stepbytes;
        step_size = ((step_size as f64) * report.stepfactor) as u64;
    }

    println!("Internal report:");
    println!("{:?}", report);

    let csv_data = benchmark_report_to_csv(&report);

    if let Some(csv_output) = cli.csv_output {
        let mut file = std::fs::File::create(csv_output).unwrap();
        file.write_all(csv_data.as_bytes()).unwrap();
    }
    else {
        println!("CSV data:");
        println!("{}", csv_data);
    }

    return ExitCode::SUCCESS
}
