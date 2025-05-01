use std::collections::HashMap;
use std::process::ExitCode;

use clap::Parser;

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

    /// csv output
    //#[arg[long]]
    //csv_output: Option<path::PathBuf>,

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
    let mut test_result_map = HashMap::<(&str, usize), TestResult>::new();
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
            let fixture_result = (test.perform_test_scenario_func)(&cli.reference_scanmem_program, &cli.test_scanmem_program, synthetic_load_path.to_str().unwrap(), 0, fixture_index, cli.verbose);
            test_result_map.insert((&test.name, fixture_index), fixture_result);
            if cli.verbose {
                println!("Ending test: {}", test_id_string);
            }
        }
    }

    let test_count = test_result_map.len();
    let pass_count = test_result_map.iter().filter(|e|*e.1 == TestResult::Pass).count();
    let fail_count = test_result_map.iter().filter(|e|*e.1 == TestResult::Fail).count();

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

            println!("{}: {}", utils::test_result_to_string(&test_result_map[&(test.name.as_str(), fixture_index)]), create_test_id_string(test.name.as_str(), fixture_index));
        }
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
