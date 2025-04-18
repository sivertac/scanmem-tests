use std::{process::ExitCode};

use clap::Parser;

use framework::{*, scanmem_driver::{self, MatchData}, synthetic_load_driver, utils::TestResult};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Path to reference scanmem program to compare to.
    #[arg(long)]
    reference_scanmem_program: String,

    /// Path to test scanmem program to evaluate.
    #[arg(long)]
    test_scanmem_program: String,

    /// Number of threads scanmem will use to scan, set to 0 to autodetect. 
    #[arg(short = 't', long, default_value_t = 0)]
    nthreads: u32,

    /// Timeout test if time elapsed is longer than specified (in seconds), 0 disables timeout.
    //#[arg(short = 'T', long, default_value_t = 0)]
    //timeout: u64,

    #[arg(long, default_value_t = 0x1u64)]
    synthetic_load_random_seed: u64,

    /// List available tests and exit.
    #[arg(short = 'l', long, default_value_t = false)]
    list_tests: bool,

    /// Specify test to run, if this is not set the test suite will run all tests.
    #[arg(long)]
    test: Option<String>,

    /// csv output
    //#[arg[long]]
    //csv_output: Option<path::PathBuf>,

    /// Echo child process stdout and stderr in parent stdout and stderr.
    #[arg(short = 'v', long, default_value_t = false)]
    verbose: bool,
}

type TestScenarioFunc = fn(reference_scanmem_program: &str, test_scanmem_program: &str, synthetic_load_program: &str, synthetic_load_random_seed: u64, nthreads: u32, verbose: bool) -> TestResult;

/// Returns match count on success.
fn test_search_regions_scanmem_part(scanmem_program: &str, target_pid: u32, nthreads: u32, verbose: bool) -> Result<u64, String> {
    // Create scanmem child process
    let mut scanmem_process = scanmem_driver::ScanmemDriver::create(scanmem_program, target_pid, nthreads, verbose).unwrap();

    scanmem_process.write_line_stdin("= 1").unwrap();
    let match_data: MatchData = scanmem_process.read_match_data();
    
    scanmem_process.write_line_stdin("exit").unwrap();

    let scanmem_exit_status = scanmem_process.wait().unwrap();

    if !scanmem_exit_status.success() {
        return Err(format!("Error: scanmem did not exit successfully, ExitStatus = {} ({})", scanmem_exit_status.code().unwrap(), scanmem_exit_status));
    }

    if !match_data.error {
        Ok(match_data.match_count)
    }
    else {
        Err("Error: Interactive error detected during execution of scanmem, look at stderr output for more info".into())
    }
}

fn scenario_func_test_search_regions(reference_scanmem_program: &str, test_scanmem_program: &str, synthetic_load_program: &str, synthetic_load_random_seed: u64, nthreads: u32, verbose: bool) -> TestResult {
    
    const SYNTHETIC_LOAD_SIZE: usize = 0x1_000_000usize;

    // Create synthetic_load child process and init.
    let mut synthetic_load_process = synthetic_load_driver::SyntheticLoadDriver::create(synthetic_load_program, verbose).unwrap();
    let synthetic_load_process_pid = synthetic_load_process.get_pid();

    // Init synthetic_load.
    synthetic_load_process.command_set_memory_size(SYNTHETIC_LOAD_SIZE).unwrap();
    synthetic_load_process.command_fill_random(synthetic_load_random_seed).unwrap();

    // Run test
    let reference_res = test_search_regions_scanmem_part(reference_scanmem_program, synthetic_load_process_pid, nthreads, verbose);
    if let Err(s) = reference_res {
        println!("{}", s);
        return TestResult::Fail;
    }
    let reference_match_count = reference_res.unwrap();

    let test_res = test_search_regions_scanmem_part(test_scanmem_program, synthetic_load_process_pid, nthreads, verbose);
    if let Err(s) = test_res {
        println!("{}", s);
        return TestResult::Fail;
    }
    let test_match_count = test_res.unwrap();

    let mut test_result = TestResult::Pass;

    expect_eq_r!(test_result, test_match_count, reference_match_count);

    // Exit synthetic_load.
    synthetic_load_process.command_exit().unwrap();

    let synthetic_load_exit_status = synthetic_load_process.wait().unwrap();
    if !synthetic_load_exit_status.success() {
        println!("Error: synthetic_load did not exit successfully, ExitStatus = {} ({})", synthetic_load_exit_status.code().unwrap(), synthetic_load_exit_status);
        test_result = TestResult::Fail;
    }

    test_result
}

fn scenario_func_test_check_matches(reference_scanmem_program: &str, test_scanmem_program: &str, synthetic_load_program: &str, _synthetic_load_random_seed: u64, nthreads: u32, verbose: bool) -> TestResult {
    const SYNTHETIC_LOAD_SIZE: usize = 0x1_000_000usize;

    // Create synthetic_load child process and init.
    let mut synthetic_load_process = synthetic_load_driver::SyntheticLoadDriver::create(synthetic_load_program, verbose).unwrap();
    let synthetic_load_process_pid = synthetic_load_process.get_pid();

    // Init synthetic_load, fill with 1s.
    synthetic_load_process.command_set_memory_size(SYNTHETIC_LOAD_SIZE).unwrap();
    synthetic_load_process.command_fill(0x1).unwrap();


    // Run test

    // Create scanmem child processes.
    let mut reference_scanmem = scanmem_driver::ScanmemDriver::create(reference_scanmem_program, synthetic_load_process_pid, nthreads, verbose).unwrap();
    let mut test_scanmem = scanmem_driver::ScanmemDriver::create(test_scanmem_program, synthetic_load_process_pid, nthreads, verbose).unwrap();
    let mut test_result = TestResult::Pass;

    {
        // Perform initial search regions, find all 1s, and read match data so we know the operation is complete.
        // We can't attach 2 times to the same process at the same time, so we need to make sure we're done scanning before attaching the other scanmem process.
        reference_scanmem.write_line_stdin("= 1").unwrap();
        let reference_match_data: MatchData = reference_scanmem.read_match_data();
        test_scanmem.write_line_stdin("= 1").unwrap();
        let test_match_data: MatchData = test_scanmem.read_match_data();
        // Validate first search regions even though we're not testing this explicitly.
        expect_eq_r!(test_result, reference_match_data.error, false);
        expect_eq_r!(test_result, test_match_data.error, false);
        expect_ge_r!(test_result, reference_match_data.match_count, SYNTHETIC_LOAD_SIZE as u64);
        expect_ge_r!(test_result, test_match_data.match_count, SYNTHETIC_LOAD_SIZE as u64);
        expect_eq_r!(test_result, reference_match_data.match_count, test_match_data.match_count);
    }

    // Mofify synthetic load to contain 2s.
    synthetic_load_process.command_fill(0x2).unwrap();

    {
        // Perform initial search regions, find all 1s, and read match data so we know the operation is complete.
        // We can't attach 2 times to the same process at the same time, so we need to make sure we're done scanning before attaching the other scanmem process. 
        reference_scanmem.write_line_stdin("= 2").unwrap();
        let reference_match_data: MatchData = reference_scanmem.read_match_data();
        test_scanmem.write_line_stdin("= 2").unwrap();
        let test_match_data: MatchData = test_scanmem.read_match_data();
        // Validate first search regions even though we're not testing this explicitly.
        expect_eq_r!(test_result, reference_match_data.error, false);
        expect_eq_r!(test_result, test_match_data.error, false);
        expect_ge_r!(test_result, reference_match_data.match_count, SYNTHETIC_LOAD_SIZE as u64);
        expect_ge_r!(test_result, test_match_data.match_count, SYNTHETIC_LOAD_SIZE as u64);
        expect_eq_r!(test_result, reference_match_data.match_count, test_match_data.match_count);
    }

    // Cleanup.
    reference_scanmem.write_line_stdin("exit").unwrap();
    test_scanmem.write_line_stdin("exit").unwrap();
    let reference_scanmem_exit_status = reference_scanmem.wait().unwrap();
    let test_scanmem_exit_status = test_scanmem.wait().unwrap();
    if !reference_scanmem_exit_status.success() {
        println!("Error: scanmem did not exit successfully, ExitStatus = {} ({})", reference_scanmem_exit_status.code().unwrap(), reference_scanmem_exit_status);
        test_result = TestResult::Fail;
    }
    if !test_scanmem_exit_status.success() {
        println!("Error: scanmem did not exit successfully, ExitStatus = {} ({})", test_scanmem_exit_status.code().unwrap(), test_scanmem_exit_status);
        test_result = TestResult::Fail;
    }

    // Exit synthetic_load.
    synthetic_load_process.command_exit().unwrap();

    let synthetic_load_exit_status = synthetic_load_process.wait().unwrap();
    if !synthetic_load_exit_status.success() {
        println!("Error: synthetic_load did not exit successfully, ExitStatus = {} ({})", synthetic_load_exit_status.code().unwrap(), synthetic_load_exit_status);
        test_result = TestResult::Fail;
    }

    test_result
}

struct TestScenario {
    name: String,
    description: String,
    perform_benchmark_scenario_func: TestScenarioFunc,
}

fn test_result_to_string(test_result: &TestResult) -> String {
    match test_result {
        TestResult::Fail => "Fail".into(),
        TestResult::Pass => "Pass".into()
    }
}

fn main() -> ExitCode {

    let cli = Cli::parse();

    let mut test_list: Vec<TestScenario> = vec![
        TestScenario{
            name: "SearchRegions".into(),
            description: "Fill target process with random bytes, then call scanmem with \"= 1; q;\". Compare matches found to reference.".into(),
            perform_benchmark_scenario_func: scenario_func_test_search_regions
        },
        TestScenario{
            name: "CheckMatches".into(),
            description: "Fill target process with 1s, and call scanmem with \"= 1\". Then modify target process to contain 2s, and call scanmem with \"= 2\". Compare matches found to reference.".into(),
            perform_benchmark_scenario_func: scenario_func_test_check_matches
        },
    ];

    // Filter tests if necessary
    if let Some(selected_test_name) = cli.test.as_ref() {
        test_list.retain(|t|t.name == *selected_test_name);
    }

    if cli.list_tests {

        for e in test_list {
            println!("{}", e.name);
            println!("\t{}", e.description);
        }

        return ExitCode::SUCCESS;
    }

    let synthetic_load_path = std::env::current_exe().unwrap().parent().unwrap().to_path_buf().join(synthetic_load_driver::SYNTHETIC_LOAD_NAME);
    
    // Run tests.
    let test_count = test_list.len();
    let mut test_result_list = vec![];
    for test in &test_list {
        // Execute test
        if cli.verbose {
            println!("Starting test: {}", test.name);
        }
        let test_result = (test.perform_benchmark_scenario_func)(&cli.reference_scanmem_program, &cli.test_scanmem_program, synthetic_load_path.to_str().unwrap(), 0, cli.nthreads, cli.verbose);
        test_result_list.push(test_result);
        if cli.verbose {
            println!("Ending test: {}", test.name);
        }
    }

    let pass_count = test_result_list.iter().filter(|e|**e == TestResult::Pass).count();
    let fail_count = test_result_list.iter().filter(|e|**e == TestResult::Fail).count();

    // Print results.
    println!("==================================");
    println!("Test results");
    println!("==================================");
    for i in 0..test_list.len() {
        let test_result = &test_result_list[i];
        let test_name = &test_list[i].name;

        println!("{}: {}", test_result_to_string(test_result), test_name);
    }
    println!("==================================");
    println!("Test results summary");
    println!("==================================");
    println!("Total number of tests....{}", test_count);
    println!("Pass count...............{}", pass_count);
    println!("Fail count...............{}", fail_count);

    if fail_count > 0 {
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}
