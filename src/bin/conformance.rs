use std::collections::{HashMap, HashSet};
use std::process::ExitCode;
use std::time::{Duration, SystemTime};
use std::path;

use clap::Parser;

use serde_json::json;

use scanmem_tests::framework::synthetic_load_driver::SYNTHETIC_LOAD_NAME;
use scanmem_tests::framework::utils::TestResult;
use scanmem_tests::framework::utils;
use scanmem_tests::*;


#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Path to reference scanmem program to compare to.
    #[arg(long)]
    reference_scanmem_program: String,

    /// Path to test scanmem program to evaluate.
    #[arg(long)]
    test_scanmem_program: String,

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

    /// CTRF output file.
    #[arg(long)]
    ctrf_output: Option<path::PathBuf>,

    /// Echo child process stdout and stderr in parent stdout and stderr.
    #[arg(short = 'v', long, default_value_t = false)]
    verbose: bool,
}

fn create_test_id_string(test_name: &str, fixture_index: usize) -> String {
    format!("{}.{}", test_name, fixture_index)
}

fn parse_test_id_string(test_id_string: &str) -> (Option<String>, Option<usize>) {
    if test_id_string.is_empty() {
        return (None, None);
    }

    let parts: Vec<&str> = test_id_string.rsplitn(2, '.').collect();

    match parts.as_slice() {
        [index_str, name] => {
            if let Ok(index) = index_str.parse::<usize>() {
                Some(name.to_string()).map_or((None, None), |n| (Some(n), Some(index)))
            } else {
                (None, None)
            }
        }
        [name] => (Some(name.to_string()), None),
        _ => (None, None),
    }
}

struct TestResultData {
    test_result: TestResult,
    execution_time: Duration,
}

// Create CTRF (Common Test Report Format) report of test result https://www.ctrf.io/.
fn create_ctrf_report(start_time: SystemTime, stop_time: SystemTime, test_count: usize, pass_count: usize, fail_count: usize, test_result_map: &HashMap::<(&str, usize), TestResultData>) -> serde_json::Value {

    let mut output_tests = vec![];

    let mut sorted_test_result_map: Vec<(&(&str, usize), &TestResultData)> = test_result_map.iter().collect();
    // Sort test results by test name and fixture. 
    // Since sort_by_key is stable, we know elements will not be reordered if they are equal, so we can sort them separately for each component.
    sorted_test_result_map.sort_by_key(|p| p.0.1);
    sorted_test_result_map.sort_by_key(|p| p.0.0);
    for p in sorted_test_result_map {

        output_tests.push(json!({
            "name": create_test_id_string(p.0.0, p.0.1),
            "suite": p.0.0,
            "status": match p.1.test_result {
                TestResult::Pass => "passed",
                TestResult::Fail => "failed", 
            },
            "duration": p.1.execution_time.as_millis()
        }));
    }

    // Count unique test names.
    let suites = test_result_map.iter().map(|e| e.0.0 ).collect::<HashSet<_>>().len();

    let output = json!({
        "reportFormat": "CTRF",
        "specVersion": "0.0.0",
        "results": {
            "tool": {
                "name": "scanmem-tests",
                "version": "0.0.0"
            },
            "summary": {
                "tests": test_count,
                "passed": pass_count,
                "failed": fail_count,
                "pending": 0,
                "skipped": 0,
                "other": 0,
                "suites": suites,
                "start": start_time.duration_since(std::time::UNIX_EPOCH).unwrap().as_millis(),
                "stop": stop_time.duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()
            },
            "tests": output_tests
        }
    });

    return output
}

fn write_json_to_file(filename: &path::PathBuf, data: &serde_json::Value) {
    let file = std::fs::File::create(filename).expect("Failed to create file");
    let writer = std::io::BufWriter::new(file);
    serde_json::to_writer_pretty(writer, data).expect("Failed to write JSON to file");
}


fn main() -> ExitCode {

    let cli = Cli::parse();

    let test_list = conformance_suite::get_test_list();

    // Filter tests if necessary
    let mut selected_test_name= None;
    let mut selected_test_fixture = None;

    if let Some(test) = cli.test.as_ref() {
        (selected_test_name, selected_test_fixture) = parse_test_id_string(test);
        if selected_test_name.is_none() && selected_test_fixture.is_none() {
            println!("Invalid test string: \"{test}\".");
            return ExitCode::FAILURE;
        }
    }

    if cli.list_tests {

        for e in test_list {
            println!("{}", e.name);
            println!("\t{}", e.description);
        }

        return ExitCode::SUCCESS;
    }

    let synthetic_load_path = std::env::current_exe().unwrap().parent().unwrap().to_path_buf().join(SYNTHETIC_LOAD_NAME);
    
    // Run tests.
    let start_time = SystemTime::now();

    let mut test_result_map = HashMap::<(&str, usize), TestResultData>::new();
    for test in &test_list {
        if let Some(e) = &selected_test_name {
            if *e != test.name {
                continue;
            }
        }
        
        for fixture_index in 0..test.fixture_count {
            if let Some(e) = &selected_test_fixture {
                if *e != fixture_index {
                    continue;
                }
            }

            let test_id_string = create_test_id_string(&test.name, fixture_index);

            // Execute test
            if cli.verbose {
                println!("Starting test: {}", test_id_string);
            }
            let test_start_time = SystemTime::now();
            let fixture_result = (test.perform_test_scenario_func)(&cli.reference_scanmem_program, &cli.test_scanmem_program, synthetic_load_path.to_str().unwrap(), cli.synthetic_load_random_seed, fixture_index, cli.verbose);
            let test_duration = SystemTime::now().duration_since(test_start_time).unwrap(); 
            test_result_map.insert((&test.name, fixture_index), TestResultData { test_result: fixture_result, execution_time: test_duration });
            if cli.verbose {
                println!("Ending test: {}", test_id_string);
            }
        }
    }
    let stop_time = SystemTime::now();

    let test_count = test_result_map.len();
    let pass_count = test_result_map.iter().filter(|e|e.1.test_result == TestResult::Pass).count();
    let fail_count = test_result_map.iter().filter(|e|e.1.test_result == TestResult::Fail).count();

    // Print results.
    println!("==================================");
    println!("Test results");
    println!("==================================");
    for test in &test_list {
        if let Some(e) = &selected_test_name {
            if *e != test.name {
                continue;
            }
        }
        for fixture_index in 0..test.fixture_count {
            if let Some(e) = &selected_test_fixture {
                if *e != fixture_index {
                    continue;
                }
            }

            println!("{}: {}", utils::test_result_to_string(&test_result_map[&(test.name.as_str(), fixture_index)].test_result), create_test_id_string(test.name.as_str(), fixture_index));
        }
    }
    println!("==================================");
    println!("Test results summary");
    println!("==================================");
    println!("Total number of tests....{}", test_count);
    println!("Pass count...............{}", pass_count);
    println!("Fail count...............{}", fail_count);

    if let Some(ctrf_output_file) = cli.ctrf_output {
        write_json_to_file(&ctrf_output_file, &create_ctrf_report(start_time, stop_time, test_count, pass_count, fail_count, &test_result_map));
    }

    if fail_count > 0 {
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}
