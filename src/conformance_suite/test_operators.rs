use crate::framework::utils::TestResult;
use crate::framework::synthetic_load_driver;
use crate::framework::scanmem_driver::{MatchData, scanmem_data_type_to_bytes};
use crate::framework::scanmem_driver;
use crate::*;

pub type ValidateFunc = fn(test_result: &mut TestResult, test_match_data: &MatchData, reference_match_data: &MatchData, thread_config: u32);

#[derive(Debug)]
struct TestData {
    scan_data_type: String,

    scan_command: String,

    validate_func: ValidateFunc,

    synthetic_load_size: usize,

    test_iterations: usize,
    
    thread_configs: Vec<u32>,
}

fn test_operator(reference_scanmem_program: &str, test_scanmem_program: &str, synthetic_load_program: &str, synthetic_load_random_seed: u64, test_data: &TestData, verbose: bool) -> TestResult {
    
    if verbose {
        println!("{:?}", test_data);
    }

    // Create synthetic_load child process and init.
    let mut synthetic_load_process = synthetic_load_driver::SyntheticLoadDriver::create(synthetic_load_program, verbose).unwrap();
    let synthetic_load_process_pid = synthetic_load_process.get_pid();

    // Create reference scanmem child processe, we only need one since we assume it is correct.
    let mut reference_scanmem = scanmem_driver::ScanmemDriver::create(reference_scanmem_program, synthetic_load_process_pid, 0, verbose).unwrap();

    // Create test scanmem processes.
    let mut test_scanmem_list = vec![];
    for nthreads in &test_data.thread_configs {
        let test_scanmem = scanmem_driver::ScanmemDriver::create(test_scanmem_program, synthetic_load_process_pid, *nthreads, verbose).unwrap();
        test_scanmem_list.push(test_scanmem);
    }

    // Set scan type.
    let configure_command = format!("option scan_data_type {}", test_data.scan_data_type);
    reference_scanmem.write_line_stdin(configure_command.as_str()).unwrap();
    for test_scanmem in &mut test_scanmem_list {
        test_scanmem.write_line_stdin(configure_command.as_str()).unwrap();
    }

    // Run test.

    let mut test_result = TestResult::Pass;

    // Init synthetic_load.
    synthetic_load_process.command_set_memory_size(test_data.synthetic_load_size).unwrap();
    synthetic_load_process.command_fill_random(synthetic_load_random_seed).unwrap();
    
    // To make sure the memory of the target process is the same between scans, manually stop the target process before we attach scanmem processes.
    synthetic_load_process.send_sigstop().unwrap();

    
    let reset_command = "reset".to_string();
    reference_scanmem.write_line_stdin(reset_command.as_str()).unwrap();
    for test_scanmem in &mut test_scanmem_list {
        test_scanmem.write_line_stdin(reset_command.as_str()).unwrap();
    }

    // Do snapshot first, so operators have something to compare against.
    {
        let scan_command = format!("snapshot");
        reference_scanmem.write_line_stdin(scan_command.as_str()).unwrap();
        let reference_match_data: MatchData = reference_scanmem.read_match_data();

        for i in 0..test_data.thread_configs.len() {
            let test_scanmem = &mut test_scanmem_list[i];
            test_scanmem.write_line_stdin(scan_command.as_str()).unwrap();
            let test_match_data: MatchData = test_scanmem.read_match_data();
            // Validate.
            (test_data.validate_func)(&mut test_result, &test_match_data, &reference_match_data, test_data.thread_configs[i]);
        }
    }

    synthetic_load_process.send_sigcont().unwrap();


    for i in 0..test_data.test_iterations {
        
        // Init synthetic_load.
        // Increase seed such that it's not the same as previous iterations.
        synthetic_load_process.command_fill_random(synthetic_load_random_seed + (i + 1) as u64).unwrap();
        
        // To make sure the memory of the target process is the same between scans, manually stop the target process before we attach scanmem processes.
        synthetic_load_process.send_sigstop().unwrap();

        // Scan.
        let scan_command = format!("{}", test_data.scan_command);
        reference_scanmem.write_line_stdin(scan_command.as_str()).unwrap();
        let reference_match_data: MatchData = reference_scanmem.read_match_data();

        for i in 0..test_data.thread_configs.len() {
            let test_scanmem = &mut test_scanmem_list[i];
            test_scanmem.write_line_stdin(scan_command.as_str()).unwrap();
            let test_match_data: MatchData = test_scanmem.read_match_data();
            // Validate.
            (test_data.validate_func)(&mut test_result, &test_match_data, &reference_match_data, test_data.thread_configs[i]);
        }

        // Resume synthetic_load
        synthetic_load_process.send_sigcont().unwrap();    
    }

    // Cleanup.
    reference_scanmem.write_line_stdin("exit").unwrap();
    for test_scanmem in &mut test_scanmem_list {
        test_scanmem.write_line_stdin("exit").unwrap();
    }
    let reference_scanmem_exit_status = reference_scanmem.wait().unwrap();
    if !reference_scanmem_exit_status.success() {
        println!("Error: scanmem did not exit successfully, ExitStatus = {} ({})", reference_scanmem_exit_status.code().unwrap(), reference_scanmem_exit_status);
        test_result = TestResult::Fail;
    }
    for test_scanmem in &mut test_scanmem_list {
        let test_scanmem_exit_status = test_scanmem.wait().unwrap();
        if !test_scanmem_exit_status.success() {
            println!("Error: scanmem did not exit successfully, ExitStatus = {} ({})", test_scanmem_exit_status.code().unwrap(), test_scanmem_exit_status);
            test_result = TestResult::Fail;
        }
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

fn validate_equal(test_result: &mut TestResult, test_match_data: &MatchData, reference_match_data: &MatchData, thread_config: u32) {
    expect_eq_r!(*test_result, reference_match_data.error, false, format!("nthreads {} failed", thread_config));
    expect_eq_r!(*test_result, test_match_data.error, false, format!("nthreads {} failed", thread_config));
    expect_eq_r!(*test_result, test_match_data.match_count, reference_match_data.match_count, format!("nthreads {} failed", thread_config));
}

fn test_operators_get_fixtures() -> Vec<TestData> {
    
    // How many threads to use.
    const THREAD_COUNT_ARRAY: [u32; 6] = [
        1, 2, 3, 11, 20, 32
    ];

    const TEST_ITERATIONS: usize = 2;

    const SYNTHETIC_LOAD_SIZE0: usize = 0x1_000_000usize;

    const DATA_TYPES: [&str; 9] = [
        "number",
        "int",
        "float",
        "int8",
        "int16",
        "int32",
        "int64",
        "float32",
        "float64",
    ];

    const OPERATORS: [(&str, ValidateFunc); 6] = [
        ("-", validate_equal),
        ("+", validate_equal),
        (">", validate_equal),
        ("<", validate_equal),
        ("!=", validate_equal),
        ("=", validate_equal),
    ];
    
    let mut ret = vec![];

    for op in OPERATORS {
        for t in DATA_TYPES {
            ret.push(TestData { scan_data_type: t.into(), scan_command: op.0.into(), validate_func: op.1, synthetic_load_size: SYNTHETIC_LOAD_SIZE0, test_iterations: TEST_ITERATIONS, thread_configs: THREAD_COUNT_ARRAY.into()});
        }
    }
    
    ret
}

pub fn scenario_func_test_operators_get_fixture_count() -> usize {
    test_operators_get_fixtures().len()
}

pub fn scenario_func_test_operators(reference_scanmem_program: &str, test_scanmem_program: &str, synthetic_load_program: &str, synthetic_load_random_seed: u64, fixture_index: usize, verbose: bool) -> TestResult {

    let test_fixtures = test_operators_get_fixtures();

    test_operator(reference_scanmem_program, test_scanmem_program, synthetic_load_program, synthetic_load_random_seed, &test_fixtures[fixture_index], verbose)
}