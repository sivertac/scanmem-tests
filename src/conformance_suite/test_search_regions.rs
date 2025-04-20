
use crate::framework::utils::TestResult;
use crate::framework::synthetic_load_driver;
use crate::framework::scanmem_driver::MatchData;
use crate::framework::scanmem_driver;
use crate::*;

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

pub fn scenario_func_test_search_regions(reference_scanmem_program: &str, test_scanmem_program: &str, synthetic_load_program: &str, synthetic_load_random_seed: u64, fixture_index: usize, verbose: bool) -> TestResult {
    
    // How many threads to use.
    const FIXTURE_DATA: [u32; 6] = [
        1, 2, 3, 11, 20, 32
    ];
    let nthreads = FIXTURE_DATA[fixture_index];

    const SYNTHETIC_LOAD_SIZE: usize = 0x1_000_000usize;

    // Create synthetic_load child process and init.
    let mut synthetic_load_process = synthetic_load_driver::SyntheticLoadDriver::create(synthetic_load_program, verbose).unwrap();
    let synthetic_load_process_pid = synthetic_load_process.get_pid();

    // Init synthetic_load.
    synthetic_load_process.command_set_memory_size(SYNTHETIC_LOAD_SIZE).unwrap();
    synthetic_load_process.command_fill_random(synthetic_load_random_seed).unwrap();
    
    // To make sure the memory of the target process is the same between scans, manually stop the target process before we attach scanmem processes.
    synthetic_load_process.send_sigstop().unwrap();
    
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

    // Resume target process such that it can close gracefully.
    synthetic_load_process.send_sigcont().unwrap();
    // Exit synthetic_load.
    synthetic_load_process.command_exit().unwrap();

    let synthetic_load_exit_status = synthetic_load_process.wait().unwrap();
    if !synthetic_load_exit_status.success() {
        println!("Error: synthetic_load did not exit successfully, ExitStatus = {} ({})", synthetic_load_exit_status.code().unwrap(), synthetic_load_exit_status);
        test_result = TestResult::Fail;
    }

    test_result
}
