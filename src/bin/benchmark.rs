
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
    #[arg(long)]
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

struct ChildProcess {
    child_process: Child,
    stdin: BufWriter<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    stderr: BufReader<ChildStderr>,
    echo: bool,
}

impl ChildProcess {
    fn new(command: &str, args: &str, echo: bool) -> Result<ChildProcess, String> {
        let args_vec: Vec<&str> = args.split_ascii_whitespace().collect();

        let mut c = match Command::new(command).args(args_vec).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn() {
            Ok(c) => c,
            Err(e) => {
                return Err(e.to_string())    
            }
        };
        let stdin = BufWriter::new(c.stdin.take().unwrap());
        let stdout = BufReader::new(c.stdout.take().unwrap());
        let stderr = BufReader::new(c.stderr.take().unwrap());

        return Ok(ChildProcess{child_process: c, stdin: stdin, stdout: stdout, stderr: stderr, echo: echo})
    }

    /// Read 1 line from stdout and return it.
    fn read_line_stdout(&mut self) -> Result<String, String> {
        let mut buf = String::new();
        self.stdout.read_line(&mut buf).map_err(|e|e.to_string())?;
        if self.echo {
            print!("pid {} stdout: {}", self.child_process.id(), buf);
        }
        return Ok(buf)
    }

    /// Read 1 line from stderr and return it.
    fn read_line_stderr(&mut self) -> Result<String, String> {
        let mut buf = String::new();
        self.stderr.read_line(&mut buf).map_err(|e|e.to_string())?;
        if self.echo {
            print!("pid {} stderr: {}", self.child_process.id(), buf);
        }
        return Ok(buf)
    }

    /// Read from stdout until exact line is present.
    /// Discards read lines.
    fn read_until_line_stdout(&mut self, condition_line: &str) -> Result<(), String> {
        loop {
            let buf = self.read_line_stdout()?;
            if buf.eq(format!("{}\n", condition_line).as_str()) {
                return Ok(())
            }
        }
    }

    fn write_line(&mut self, line: &str) -> Result<(), String> {
        let out = format!("{}\n", line);
        if self.echo {
            print!("pid {} stdin: {}", self.child_process.id(), out);
        }
        self.stdin.write_all(out.as_bytes()).map_err(|e|e.to_string())?;
        self.stdin.flush().map_err(|e|e.to_string())?;
        return Ok(())
    }
}

impl Drop for ChildProcess {
    fn drop(&mut self) {
        if self.echo {
            // Read whats left in the output pipes
            loop {
                let buf = self.read_line_stdout().unwrap();                
                if buf.len() == 0 {
                    break;
                }
            }
            loop {
                let buf = self.read_line_stderr().unwrap();
                if buf.len() == 0 {
                    break;
                }
            }
        }
        println!("Dropping ChildProcess pid {}", self.child_process.id());
    }
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
    let mut scanmem = ChildProcess::new(scanmem_program, args.as_str(), verbose)?;
    for command in scanmem_commands {
        scanmem.write_line(command)?;
    }
    
    // Cleanup
    let scanmem_exit_status = scanmem.child_process.wait().unwrap();
    if !scanmem_exit_status.success() {
        return Err(format!("Error: scanmem did not exit successfully, ExitStatus = {} ({})", scanmem_exit_status.code().unwrap(), scanmem_exit_status.to_string()));
    }
    println!("scanmem child process done");
    
    return Ok(())
}

fn scenario_func_fill_random(scanmem_program: &str, synthetic_load_program: &str, synthetic_load_size: u64, synthetic_load_random_seed: u64, iterations: usize, nthreads: i32, verbose: bool) -> Result<BenchmarkTiming, String> {

    let scanmem_commands = vec!["= 1", "q"];

    let mut report = BenchmarkTiming::default();

    let total_start_time = SystemTime::now();

    // Create synthetic_load child process and init
    println!("Starting synthetic_load child process...");
    let mut synthetic_load = ChildProcess::new(synthetic_load_program, "", verbose)?;
    println!("Child pid: {}", synthetic_load.child_process.id());
    synthetic_load.write_line(format!("set-memory-size {}", synthetic_load_size).as_str())?;
    synthetic_load.read_until_line_stdout("Done")?;
    synthetic_load.write_line(format!("fill-random {}", synthetic_load_random_seed).as_str())?;
    synthetic_load.read_until_line_stdout("Done")?;

    
    report.setup_time = SystemTime::now().duration_since(total_start_time).map_err(|e|e.to_string())?;

    report.benchmark_times.reserve(iterations);
    for _ in 0..iterations {
        let start = SystemTime::now();
        scenario_func_fill_random_iteration(scanmem_program, &scanmem_commands, synthetic_load.child_process.id(), nthreads, verbose)?;
        report.benchmark_times.push(SystemTime::now().duration_since(start).map_err(|e|e.to_string())?)
    }

    synthetic_load.write_line(format!("exit").as_str())?;
    let synthetic_load_exit_status = synthetic_load.child_process.wait().unwrap();
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
