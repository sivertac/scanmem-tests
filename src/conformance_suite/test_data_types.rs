
use crate::framework::utils::TestResult;
use crate::framework::synthetic_load_driver;
use crate::framework::scanmem_driver::MatchData;
use crate::framework::scanmem_driver;
use crate::*;

pub const TEST_DATA_TYPES_FIXED_SIZE_FIXTURE_COUNT: usize = 9;

fn data_type_to_bytearray(data_type: &str, value: &str) -> Vec<u8> {
    match data_type {
        "number" => {
            let v: i64 = value.parse().unwrap(); 
            v.to_le_bytes().to_vec()
        },
        "int" => {
            let v: i32 = value.parse().unwrap(); 
            v.to_le_bytes().to_vec()
        },
        "float" => {
            let v: f64 = value.parse().unwrap(); 
            v.to_le_bytes().to_vec()
        },
        "int8" => {
            let v: i8 = value.parse().unwrap(); 
            v.to_le_bytes().to_vec()
        },
        "int16" => {
            let v: i16 = value.parse().unwrap(); 
            v.to_le_bytes().to_vec()
        },
        "int32" => {
            let v: i32 = value.parse().unwrap(); 
            v.to_le_bytes().to_vec()
        },
        "int64" => {
            let v: i64 = value.parse().unwrap(); 
            v.to_le_bytes().to_vec()
        },
        "float32" => {
            let v: f32 = value.parse().unwrap(); 
            v.to_le_bytes().to_vec()
        },
        "float64" => {
            let v: f64 = value.parse().unwrap(); 
            v.to_le_bytes().to_vec()
        }   , 
        _ => {
            assert!(false);
            vec![]
        }
    }
}

pub fn scenario_func_test_data_types_fixed_size(reference_scanmem_program: &str, test_scanmem_program: &str, synthetic_load_program: &str, _synthetic_load_random_seed: u64, fixture_index: usize, verbose: bool) -> TestResult {

    // How many threads to use.
    const THREAD_COUNT_ARRAY: [u32; 6] = [
        1, 2, 3, 11, 20, 32
    ];

    // corresponds to fixture_index, (data_type, test_value0, test_value1)
    const DATA_TYPE_ARRAY: [(&str, &str, &str); TEST_DATA_TYPES_FIXED_SIZE_FIXTURE_COUNT] = [
        ("number", "-123123123", "0"),
        ("int", "123123", "-123123123"),
        ("float", "0.123123123", "123.123123"),
        ("int8", "123", "1"),
        ("int16", "12312", "-10"),
        ("int32", "123123123", "1111"),
        ("int64", "123123123123123", "-100000000000"),
        ("float32", "-0.123123123123", "0.123123123123"),
        ("float64", "0.123123123123123123", "1.123123123123123123"),
    ];
    let (data_type_string, data_type_test_value0, data_type_test_value1)  = DATA_TYPE_ARRAY[fixture_index];

    const SYNTHETIC_LOAD_SIZE0: usize = 0x1_000_000usize;
    const SYNTHETIC_LOAD_SIZE1: usize = SYNTHETIC_LOAD_SIZE0 / 2;
    
    // Create synthetic_load child process and init.
    let mut synthetic_load_process = synthetic_load_driver::SyntheticLoadDriver::create(synthetic_load_program, verbose).unwrap();
    let synthetic_load_process_pid = synthetic_load_process.get_pid();

    // Init synthetic_load, fill with test_value_0.
    synthetic_load_process.command_set_memory_size(SYNTHETIC_LOAD_SIZE0).unwrap();
    synthetic_load_process.command_fill_bytearray(data_type_to_bytearray(data_type_string, data_type_test_value0).as_slice()).unwrap();
    
    // To make sure the memory of the target process is the same between scans, manually stop the target process before we attach scanmem processes.
    synthetic_load_process.send_sigstop().unwrap();

    // Run test

    // Create reference scanmem child processe, we only need one since we assume it is correct.
    let mut reference_scanmem = scanmem_driver::ScanmemDriver::create(reference_scanmem_program, synthetic_load_process_pid, 0, verbose).unwrap();

    // Create test scanmem processes
    let mut test_scanmem_list = vec![];
    for i in 0..THREAD_COUNT_ARRAY.len() {
        let test_scanmem = scanmem_driver::ScanmemDriver::create(test_scanmem_program, synthetic_load_process_pid, THREAD_COUNT_ARRAY[i], verbose).unwrap();
        test_scanmem_list.push(test_scanmem);
    }
    let mut test_result = TestResult::Pass;

    // set scan type
    let configure_command = format!("option scan_data_type {}", data_type_string);
    reference_scanmem.write_line_stdin(configure_command.as_str()).unwrap();
    for test_scanmem in &mut test_scanmem_list {
        test_scanmem.write_line_stdin(configure_command.as_str()).unwrap();
    }

    {
        // Perform initial search regions.
        let scan_command = format!("= {}", data_type_test_value0);
        reference_scanmem.write_line_stdin(scan_command.as_str()).unwrap();
        let reference_match_data: MatchData = reference_scanmem.read_match_data();

        for i in 0..THREAD_COUNT_ARRAY.len() {
            let test_scanmem = &mut test_scanmem_list[i];
            test_scanmem.write_line_stdin(scan_command.as_str()).unwrap();
            let test_match_data: MatchData = test_scanmem.read_match_data();
            // Validate.
            expect_eq_r!(test_result, reference_match_data.error, false, format!("nthreads {} failed", THREAD_COUNT_ARRAY[i]));
            expect_eq_r!(test_result, test_match_data.error, false, format!("nthreads {} failed", THREAD_COUNT_ARRAY[i]));
            expect_eq_r!(test_result, reference_match_data.match_count, test_match_data.match_count, format!("nthreads {} failed", THREAD_COUNT_ARRAY[i]));
        }
    }

    // Modify synthetic load to test_value_1.
    synthetic_load_process.send_sigcont().unwrap();
    synthetic_load_process.command_set_memory_size(SYNTHETIC_LOAD_SIZE1).unwrap();
    synthetic_load_process.command_fill_bytearray(data_type_to_bytearray(data_type_string, data_type_test_value1).as_slice()).unwrap();
    synthetic_load_process.send_sigstop().unwrap();

    {
        // Perform initial search regions.
        let scan_command = format!("= {}", data_type_test_value1);
        reference_scanmem.write_line_stdin(scan_command.as_str()).unwrap();
        let reference_match_data: MatchData = reference_scanmem.read_match_data();

        for i in 0..THREAD_COUNT_ARRAY.len() {
            let test_scanmem = &mut test_scanmem_list[i];
            test_scanmem.write_line_stdin(scan_command.as_str()).unwrap();
            let test_match_data: MatchData = test_scanmem.read_match_data();
            // Validate.
            expect_eq_r!(test_result, reference_match_data.error, false, format!("nthreads {} failed", THREAD_COUNT_ARRAY[i]));
            expect_eq_r!(test_result, test_match_data.error, false, format!("nthreads {} failed", THREAD_COUNT_ARRAY[i]));
            expect_eq_r!(test_result, reference_match_data.match_count, test_match_data.match_count, format!("nthreads {} failed", THREAD_COUNT_ARRAY[i]));
        }
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