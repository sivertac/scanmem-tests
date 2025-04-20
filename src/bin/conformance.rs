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

fn main() -> ExitCode {

    let cli = Cli::parse();

    let mut test_list = conformance_suite::get_test_list();

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

    let synthetic_load_path = std::env::current_exe().unwrap().parent().unwrap().to_path_buf().join(SYNTHETIC_LOAD_NAME);
    
    // Run tests.
    let mut test_result_list = vec![];
    for test in &test_list {
        let mut test_fixture_results = vec![];
        for fixture_index in 0..test.fixtures_count {
            let test_id_string = create_test_id_string(&test.name, fixture_index);

            // Execute test
            if cli.verbose {
                println!("Starting test: {}", test_id_string);
            }
            let fixture_result = (test.perform_benchmark_scenario_func)(&cli.reference_scanmem_program, &cli.test_scanmem_program, synthetic_load_path.to_str().unwrap(), 0, fixture_index, cli.verbose);
            test_fixture_results.push(fixture_result);
            if cli.verbose {
                println!("Ending test: {}", test_id_string);
            }
        }
        test_result_list.push(test_fixture_results);
    }

    let test_count = test_result_list.iter().flatten().count();
    let pass_count = test_result_list.iter().flatten().filter(|e|**e == TestResult::Pass).count();
    let fail_count = test_result_list.iter().flatten().filter(|e|**e == TestResult::Fail).count();

    // Print results.
    println!("==================================");
    println!("Test results");
    println!("==================================");
    for test_index in 0..test_list.len() {
        let test_fixture_results = &test_result_list[test_index];
        let test_name = &test_list[test_index].name;
        let fixture_count = test_list[test_index].fixtures_count;
        for fixture_index in 0..fixture_count {
            println!("{}: {}", utils::test_result_to_string(&test_fixture_results[fixture_index]), create_test_id_string(test_name, fixture_index));
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
