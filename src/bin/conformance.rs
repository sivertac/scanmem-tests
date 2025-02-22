use std::{io::{BufReader, BufWriter, Write}, path, process::{Command, ExitCode, Stdio}, time::{Duration, SystemTime}};

use clap::Parser;

mod utils;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Path to reference scanmem program to compare to.
    #[arg(long)]
    reference_scanmem_program: String,

    /// Path to test scanmem program to evaluate.
    #[arg(long)]
    test_scanmem_program: String,

    /// Number of threads scanmem will use to scan, set to -1 if multi threading is not supported by the scanmem program. 
    #[arg(short = 't', long, default_value_t = -1)]
    nthreads: i32,

    /// Timeout test if time elapsed is longer than specified (in seconds), 0 disables timeout.
    //#[arg(short = 'T', long, default_value_t = 0)]
    //timeout: u64,

    #[arg(long, default_value_t = 0x1u64)]
    synthetic_load_random_seed: u64,

    /// List available tests and exit.
    #[arg(short = 'l', long, default_value_t = false)]
    list_tests: bool,

    /// csv output
    //#[arg[long]]
    //csv_output: Option<path::PathBuf>,

    /// Echo child process stdout and stderr in parent stdout and stderr.
    #[arg(short = 'v', long, default_value_t = false)]
    verbose: bool,
}

enum TestResult {
    Pass,
    Fail,
}

type TestScenarioFunc = fn(reference_scanmem_program: &str, test_scanmem_program: &str, synthetic_load_program: &str, synthetic_load_random_seed: u64, nthreads: i32, verbose: bool) -> Result<TestResult, String>;

fn scenario_func_test_search_regions(reference_scanmem_program: &str, test_scanmem_program: &str, synthetic_load_program: &str, synthetic_load_random_seed: u64, nthreads: i32, verbose: bool) -> Result<TestResult, String> {
    
    const synthetic_load_size: u64 = 0x1_000_000u64;

    // Create synthetic_load child process and init.
    let mut synthetic_load_process = utils::SyntheticLoadProcess::create(synthetic_load_program, verbose).unwrap();
    let synthetic_load_process_pid = synthetic_load_process.get_pid();

    // Init synthetic_load.
    synthetic_load_process.write_line_stdin(format!("set-memory-size {}", synthetic_load_size).as_str()).unwrap();
    synthetic_load_process.read_until_line_stdout("Done").unwrap();
    synthetic_load_process.write_line_stdin(format!("fill-random {}", synthetic_load_random_seed).as_str()).unwrap();
    synthetic_load_process.read_until_line_stdout("Done").unwrap();


    // Run test
    {
        let scanmem_program = reference_scanmem_program;
        let scanmem_commands = vec!["= 1", "exit"];
        let mut match_count: u64 = 0;

        // Create scanmem child process
        println!("Starting scanmem child process...");
        let args: String;
        if nthreads == -1 {
            args = format!("--pid={}", synthetic_load_process_pid);
        }
        else {
            args = format!("--pid={} --jobs={}", synthetic_load_process_pid, nthreads);
        }

        let args_vec: Vec<&str> = args.split_ascii_whitespace().collect();

        let scanmem_exit_status;

        struct ThreadData {
            error: bool,
            match_count: u64,
        }

        // I don't like this.
        let stderr_data_mutex = std::sync::Arc::<std::sync::Mutex::<ThreadData>>::new(ThreadData{error: false, match_count: 0}.into());
        //let error_arc  = std::sync::Arc::<std::sync::atomic::AtomicBool>::new(false.into());

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
                utils::drain_stdout(&mut stdout, pid, verbose).unwrap();
            });

            //let error_arc_2  = error_arc.clone();
            let stderr_data_mutex = stderr_data_mutex.clone();
            let stderr_thread = std::thread::spawn(move || {
                //let error_arc_2 = error_arc_2;
                let stderr_data_mutex = stderr_data_mutex;

                loop {
                    let buf = utils::read_line_stderr(&mut stderr, pid, verbose).unwrap();
                    if buf.len() == 0 {
                        break;
                    }

                    const MATCH_COUNT_PREFIX: &str = "info: we currently have ";
                    const MATCH_COUNT_SUFFIX: &str = " matches.\n";
                    if let Some(sub_str) = buf.strip_prefix(MATCH_COUNT_PREFIX) {
                        // Store matches
                        let match_count_string = sub_str.strip_suffix(MATCH_COUNT_SUFFIX).unwrap();
                        stderr_data_mutex.lock().unwrap().match_count = match_count_string.parse().unwrap();
                    }
                    else if buf.contains("Operation not permitted") {
                        // Check if we have a permission error when running scanmem.
                        stderr_data_mutex.lock().unwrap().error = true;
                    }
                }
            });

            for command in scanmem_commands {
                utils::write_line(&mut stdin, command, pid, verbose)?;
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

        let error = stderr_data_mutex.lock().unwrap().error;
        match_count = stderr_data_mutex.lock().unwrap().match_count;

        if error {
            return Err(format!("Error: Interactive error detected during execution of scanmem, look at stderr output for more info"));
        }

        println!("scanmem child process done");
    }
    

    return Ok(TestResult::Pass);
}

struct TestScenario {
    name: String,
    description: String,
    perform_benchmark_scenario_func: TestScenarioFunc,
}

fn main() -> ExitCode {

    let cli = Cli::parse();

    let test_list: Vec<TestScenario> = vec![
        TestScenario{
            name: "SearchRegions".into(),
            description: "Fill target process with random bytes, then call scanmem with \"= 1; q;\". Compare matches found to reference.".into(),
            perform_benchmark_scenario_func: scenario_func_test_search_regions
        },
    ];

    if cli.list_tests {

        for e in test_list {
            println!("{}", e.name);
            println!("\t{}", e.description);
        }

        return ExitCode::SUCCESS;
    }

    let synthetic_load_path = std::env::current_exe().unwrap().parent().unwrap().to_path_buf().join(utils::SYNTHETIC_LOAD_NAME);
    
    // Run tests.
    for test in test_list {
        (test.perform_benchmark_scenario_func)(&cli.reference_scanmem_program, &cli.test_scanmem_program, synthetic_load_path.to_str().unwrap(), 0, cli.nthreads, cli.verbose).unwrap();
    }

    return ExitCode::SUCCESS
}
