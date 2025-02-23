
use std::{io::Write, path, process::ExitCode, time::{Duration, SystemTime}};
use clap::Parser;

mod utils;

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
    match_count: u64, // Number of matches found at end of iteration
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
    ret.push_str(&create_csv_row(&vec!["scanmem_program", "benchmark_name", "nthreads", "synthetic_load_size(bytes)", "synthetic_load_random_seed", "setup_time(ms)", "total_time(ms)", "iteration", "benchmark_time(ms)", "match_count"]));
    ret.push('\n');
    for result in &report.results {
        let synthetic_load_size = result.synthetic_load_size.to_string();
        let synthetic_load_random_seed = result.synthetic_load_random_seed.to_string();
        let setup_time = result.setup_time.as_millis().to_string();
        let total_time = result.total_time.as_millis().to_string();
        for iteration in 0..result.iterations.len() {
            let benchmark_time = result.iterations[iteration].benchmark_time.as_millis().to_string();
            let match_count = result.iterations[iteration].match_count.to_string();
            ret.push_str(&create_csv_row(&vec![scanmem_program, benchmark_name, &nthreads, &synthetic_load_size, &synthetic_load_random_seed, &setup_time, &total_time, &iteration.to_string(), &benchmark_time, &match_count]));
            ret.push('\n');
        }
    }

    return ret;
}

type BenchmarkScenarioFunc = fn(result: &mut BenchmarkResult, scanmem_program: &str, synthetic_load_program: &str, synthetic_load_size: u64, synthetic_load_random_seed: u64, iteration_count: usize, nthreads: i32, verbose: bool) -> Result<(), String>;

fn scenario_func_fill_random_iteration(scanmem_program: &str, scanmem_commands: &Vec<&str>, target_process_pid: u32, nthreads: i32, verbose: bool, match_count: &mut u64) -> Result<(), String> {
    
    // Create scanmem child process
    let mut scanmem_process = utils::ScanmemProcess::create(scanmem_program, target_process_pid, nthreads, verbose).unwrap();

    // Write commands, we assume we have only 1 scan in the program.
    for command in scanmem_commands {
        scanmem_process.write_line_stdin(command).unwrap();
    }

    // Capture match data.
    let match_data = scanmem_process.read_match_data();

    // Assume the end of the scanmem program ends with "exit".
    let scanmem_exit_status = scanmem_process.wait().unwrap();
    if !scanmem_exit_status.success() {
        return Err(format!("Error: scanmem did not exit successfully, ExitStatus = {} ({})", scanmem_exit_status.code().unwrap(), scanmem_exit_status.to_string()));
    }

    let error = match_data.error;
    *match_count = match_data.match_count;

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
    let mut synthetic_load_process = utils::SyntheticLoadProcess::create(synthetic_load_program, verbose).unwrap();
    let synthetic_load_process_pid = synthetic_load_process.get_pid();

    // Init synthetic_load.
    synthetic_load_process.write_line_stdin(format!("set-memory-size {}", synthetic_load_size).as_str()).unwrap();
    synthetic_load_process.read_until_line_stdout("Done").unwrap();
    synthetic_load_process.write_line_stdin(format!("fill-random {}", synthetic_load_random_seed).as_str()).unwrap();
    synthetic_load_process.read_until_line_stdout("Done").unwrap();

    // Run benchmark.
    result.setup_time = SystemTime::now().duration_since(total_start_time).map_err(|e|e.to_string())?;

    iterations.reserve(iteration_count);
    for _ in 0..iteration_count {
        let start = SystemTime::now();
        let mut match_count: u64 = 0;
        scenario_func_fill_random_iteration(scanmem_program, &scanmem_commands, synthetic_load_process_pid, nthreads, verbose, &mut match_count)?;
        
        let duration = SystemTime::now().duration_since(start).map_err(|e|e.to_string())?;
        iterations.push(BenchmarkIteration{benchmark_time: duration, match_count: match_count});
    }

    // Exit synthetic_load.
    synthetic_load_process.write_line_stdin(format!("exit").as_str()).unwrap();
    synthetic_load_process.drain_stdout().unwrap();

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

    let synthetic_load_path = std::env::current_exe().unwrap().parent().unwrap().to_path_buf().join(utils::SYNTHETIC_LOAD_NAME);
    
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
        benchmark_result.standard_deviation = utils::compute_standard_deviation(benchmark_result.iterations.iter().map(|e|e.benchmark_time.as_secs_f64()), benchmark_result.mean);
        benchmark_result.median = utils::compute_median(benchmark_result.iterations.iter().map(|e|e.benchmark_time.as_secs_f64()));

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
