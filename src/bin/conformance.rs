use std::process::ExitCode;

use clap::Parser;

use framework::{synthetic_load_driver, scanmem_driver, scanmem_driver::MatchData};

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

/// Returns match count on success.
fn test_search_regions_scanmem_part(scanmem_program: &str, target_pid: u32, nthreads: i32, verbose: bool) -> Result<u64, String> {
    // Create scanmem child process
    let mut scanmem_process = scanmem_driver::ScanmemDriver::create(scanmem_program, target_pid, nthreads, verbose).unwrap();

    scanmem_process.write_line_stdin("= 1").unwrap();
    let match_data: MatchData = scanmem_process.read_match_data();
    
    scanmem_process.write_line_stdin("exit").unwrap();

    let scanmem_exit_status = scanmem_process.wait().unwrap();

    if !scanmem_exit_status.success() {
        return Err(format!("Error: scanmem did not exit successfully, ExitStatus = {} ({})", scanmem_exit_status.code().unwrap(), scanmem_exit_status.to_string()));
    }

    if !match_data.error {
        return Ok(match_data.match_count);
    }
    else {
        return Err("Error: Interactive error detected during execution of scanmem, look at stderr output for more info".into());
    }
}

fn scenario_func_test_search_regions(reference_scanmem_program: &str, test_scanmem_program: &str, synthetic_load_program: &str, synthetic_load_random_seed: u64, nthreads: i32, verbose: bool) -> Result<TestResult, String> {
    
    const SYNTHETIC_LOAD_SIZE: usize = 0x1_000_000usize;

    // Create synthetic_load child process and init.
    let mut synthetic_load_process = synthetic_load_driver::SyntheticLoadDriver::create(synthetic_load_program, verbose).unwrap();
    let synthetic_load_process_pid = synthetic_load_process.get_pid();

    // Init synthetic_load.
    synthetic_load_process.command_set_memory_size(SYNTHETIC_LOAD_SIZE).unwrap();
    synthetic_load_process.command_fill_random(synthetic_load_random_seed).unwrap();


    // Run test
    let mut test_result = TestResult::Pass;

    let reference_match_count = test_search_regions_scanmem_part(reference_scanmem_program, synthetic_load_process_pid, nthreads, verbose)?;

    let test_match_count = test_search_regions_scanmem_part(reference_scanmem_program, synthetic_load_process_pid, nthreads, verbose)?;

    if test_match_count != reference_match_count {
        println!("Mismatch: test_match_count({}) != reference_match_count({})", test_match_count, reference_match_count);
        test_result = TestResult::Fail;
    }


    // Exit synthetic_load.
    synthetic_load_process.command_exit().unwrap();

    let synthetic_load_exit_status = synthetic_load_process.wait().unwrap();
    if !synthetic_load_exit_status.success() {
        return Err(format!("Error: synthetic_load did not exit successfully, ExitStatus = {} ({})", synthetic_load_exit_status.code().unwrap(), synthetic_load_exit_status.to_string()));
    }

    return Ok(test_result);
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

    let synthetic_load_path = std::env::current_exe().unwrap().parent().unwrap().to_path_buf().join(synthetic_load_driver::SYNTHETIC_LOAD_NAME);
    
    // Run tests.
    for test in test_list {
        (test.perform_benchmark_scenario_func)(&cli.reference_scanmem_program, &cli.test_scanmem_program, synthetic_load_path.to_str().unwrap(), 0, cli.nthreads, cli.verbose).unwrap();
    }

    return ExitCode::SUCCESS
}
