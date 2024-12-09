
use std::{io::{BufRead, BufReader, BufWriter, Write}, process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitCode, Stdio}, time::{Duration, SystemTime}};
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

    /// Echo child process stdout and stderr in parent stdout and stderr.
    #[arg(short = 'v', long, default_value_t = false)]
    verbose: bool,
}

#[derive(Default, Debug)]
struct BenchmarkTiming {
    setup_time: Duration,
    benchmark_times: Vec<Duration>,
    total_time: Duration
}

#[derive(Default, Debug)]
struct BenchmarkResult {
    // params
    synthetic_load_size: u64, 
    synthetic_load_random_seed: u64,
    
    // timings
    timing: BenchmarkTiming,

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
    iterations: usize,
    timeout: u64,

    // results
    results: Vec<BenchmarkResult>,
}

fn read_line_stream<S: BufRead>(stream: &mut S, pid: u32, echo: bool, name: &str) -> Result<String, String> {
    let mut buf = String::new();
    stream.read_line(&mut buf).map_err(|e|e.to_string())?;
    if echo {
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

type BenchmarkScenarioFunc = fn(scanmem_program: &str, synthetic_load_program: &str, synthetic_load_size: u64, synthetic_load_random_seed: u64, iterations: usize, nthreads: i32, verbose: bool) -> Result<BenchmarkTiming, String>;

fn scenario_func_fill_random_iteration(scanmem_program: &str, scanmem_commands: &Vec<&str>, target_process_pid: u32, nthreads: i32, verbose: bool) -> Result<(), String> {
    
    // Create scanmem child process
    println!("Starting scanmem child process...");
    let args: String;
    if nthreads == -1 {
        args = format!("--pid={}", target_process_pid);
    }
    else {
        args = format!("--pid={} -j={}", target_process_pid, nthreads);
    }

    let args_vec: Vec<&str> = args.split_ascii_whitespace().collect();

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
        loop {
            let buf = read_line_stdout(&mut stdout, pid, verbose).unwrap();
            if buf.len() == 0 {
                break;
            }
        }
    });

    let stderr_thread = std::thread::spawn(move || {
        loop {
            let buf = read_line_stderr(&mut stderr, pid, verbose).unwrap();
            if buf.len() == 0 {
                break;
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
    let scanmem_exit_status = scanmem_process.wait().unwrap();
    if !scanmem_exit_status.success() {
        return Err(format!("Error: scanmem did not exit successfully, ExitStatus = {} ({})", scanmem_exit_status.code().unwrap(), scanmem_exit_status.to_string()));
    }
    println!("scanmem child process done");
    
    return Ok(())
}

fn scenario_func_fill_random(scanmem_program: &str, synthetic_load_program: &str, synthetic_load_size: u64, synthetic_load_random_seed: u64, iterations: usize, nthreads: i32, verbose: bool) -> Result<BenchmarkTiming, String> {

    let scanmem_commands = vec!["= 1", "exit"];

    let mut report = BenchmarkTiming::default();

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
    report.setup_time = SystemTime::now().duration_since(total_start_time).map_err(|e|e.to_string())?;

    report.benchmark_times.reserve(iterations);
    for _ in 0..iterations {
        let start = SystemTime::now();
        scenario_func_fill_random_iteration(scanmem_program, &scanmem_commands, pid, nthreads, verbose)?;
        report.benchmark_times.push(SystemTime::now().duration_since(start).map_err(|e|e.to_string())?)
    }

    // Exit synthetic_load.
    write_line(&mut stdin, format!("exit").as_str(), pid, verbose)?;
    drain_stdout(&mut stdout, pid, verbose)?;

    let synthetic_load_exit_status = synthetic_load_process.wait().unwrap();
    if !synthetic_load_exit_status.success() {
        return Err(format!("Error: synthetic_load did not exit successfully, ExitStatus = {} ({})", synthetic_load_exit_status.code().unwrap(), synthetic_load_exit_status.to_string()));
    }

    report.total_time = SystemTime::now().duration_since(total_start_time).map_err(|e|e.to_string())?;

    return Ok(report)
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
    report.iterations = cli.iterations;
    report.timeout = cli.timeout;

    let mut step_size = report.minbytes;
    while step_size >= report.minbytes && step_size <= report.maxbytes {
        
        let mut benchmark_result = BenchmarkResult::default();
        benchmark_result.synthetic_load_size = step_size;
        benchmark_result.synthetic_load_random_seed = cli.synthetic_load_random_seed; 

        match (benchmark_scenario.perform_benchmark_scenario_func)(&report.scanmem_program, synthetic_load_path.to_str().unwrap(), benchmark_result.synthetic_load_size, benchmark_result.synthetic_load_random_seed, cli.iterations, report.nthreads, cli.verbose) {
            Ok(t) => benchmark_result.timing = t,
            Err(err) => {
                println!("Benchmark failed: {}", err);
            }
        }

        // compute aggregates
        benchmark_result.max = benchmark_result.timing.benchmark_times.iter().map(|e|e.as_secs_f64()).max_by(|a,b|a.total_cmp(b)).unwrap();
        benchmark_result.min = benchmark_result.timing.benchmark_times.iter().map(|e|e.as_secs_f64()).min_by(|a,b|a.total_cmp(b)).unwrap();
        benchmark_result.mean = benchmark_result.timing.benchmark_times.iter().map(|e|e.as_secs_f64()).sum::<f64>() / benchmark_result.timing.benchmark_times.len() as f64;
        benchmark_result.standard_deviation = compute_standard_deviation(benchmark_result.timing.benchmark_times.iter().map(|e|e.as_secs_f64()), benchmark_result.mean);
        benchmark_result.median = compute_median(benchmark_result.timing.benchmark_times.iter().map(|e|e.as_secs_f64()));

        report.results.push(benchmark_result);

        // next step
        step_size += report.stepbytes;
        step_size = ((step_size as f64) * report.stepfactor) as u64;
    }


    println!("{:?}", report);

    return ExitCode::SUCCESS
}
