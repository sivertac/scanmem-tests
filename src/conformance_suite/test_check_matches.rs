use crate::framework::utils::TestResult;
use crate::framework::synthetic_load_driver;
use crate::framework::scanmem_driver::MatchData;
use crate::framework::scanmem_driver;
use crate::*;

pub fn scenario_func_test_check_matches(reference_scanmem_program: &str, test_scanmem_program: &str, synthetic_load_program: &str, _synthetic_load_random_seed: u64, fixture_index: usize, verbose: bool) -> TestResult {
    const SYNTHETIC_LOAD_SIZE: usize = 0x1_000_000usize;

    // How many threads to use.
    const FIXTURE_DATA: [u32; 6] = [
        1, 2, 3, 11, 20, 32
    ];
    let nthreads = FIXTURE_DATA[fixture_index];

    // Create synthetic_load child process and init.
    let mut synthetic_load_process = synthetic_load_driver::SyntheticLoadDriver::create(synthetic_load_program, verbose).unwrap();
    let synthetic_load_process_pid = synthetic_load_process.get_pid();

    // Init synthetic_load, fill with 1s.
    synthetic_load_process.command_set_memory_size(SYNTHETIC_LOAD_SIZE).unwrap();
    synthetic_load_process.command_fill(0x1).unwrap();

    // To make sure the memory of the target process is the same between scans, manually stop the target process before we attach scanmem processes.
    synthetic_load_process.send_sigstop().unwrap();

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
    synthetic_load_process.send_sigcont().unwrap();
    synthetic_load_process.command_fill(0x2).unwrap();
    synthetic_load_process.send_sigstop().unwrap();

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