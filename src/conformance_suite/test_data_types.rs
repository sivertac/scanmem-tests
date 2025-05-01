
use crate::framework::utils::TestResult;
use crate::framework::synthetic_load_driver;
use crate::framework::scanmem_driver::MatchData;
use crate::framework::scanmem_driver;
use crate::*;

fn scanmem_data_type_to_bytes(data_type: &str, value: &str) -> Vec<u8> {
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
        },
        "bytearray" => {
            scanmem_bytearray_to_bytes(value).unwrap()
        },
        "string" => { 
            value.as_bytes().to_vec()
        },
        _ => {
            assert!(false);
            vec![]
        }
    }
}

fn bytearray_to_scanmem_input(bytearray: &[u8]) -> String {
    let mut ret = String::new();

    for v in bytearray {
        ret.push_str(format!("{:02X} ", v).as_str());
    }

    ret
}

fn scanmem_bytearray_to_bytes(input: &str) -> Result<Vec<u8>, std::num::ParseIntError> {
    input
        .split_whitespace()
        .map(|chunk| {
            if chunk == "??" {
                Ok(0u8)
            } else {
                u8::from_str_radix(chunk, 16)
            }
        })
        .collect()
}

#[derive(Debug)]
struct TestData {
    scan_data_type: String,

    scan_predicate: String,
    
    /// load size and test value per iteration (synthetic load size, test value)
    test_values: Vec<(usize, String)>,
    
    thread_configs: Vec<u32>,
}

fn test_data_types(reference_scanmem_program: &str, test_scanmem_program: &str, synthetic_load_program: &str, test_data: &TestData, verbose: bool) -> TestResult {
    
    println!("{:?}", test_data);

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

    let mut is_first_scan = true;

    for (synthetic_load_size, test_value) in &test_data.test_values {
        
        // Init synthetic_load.
        synthetic_load_process.command_set_memory_size(*synthetic_load_size).unwrap();
        synthetic_load_process.command_fill_bytearray(scanmem_data_type_to_bytes(&test_data.scan_data_type, test_value).as_slice()).unwrap();
        
        // To make sure the memory of the target process is the same between scans, manually stop the target process before we attach scanmem processes.
        synthetic_load_process.send_sigstop().unwrap();

        // If this is the first scan, we need to make sure all regions is known by scanmem, a reset will reload the memory regions.
        if is_first_scan {
            let reset_command = "reset".to_string();
            reference_scanmem.write_line_stdin(reset_command.as_str()).unwrap();
            for test_scanmem in &mut test_scanmem_list {
                test_scanmem.write_line_stdin(reset_command.as_str()).unwrap();
            }
            is_first_scan = false;
        }

        // Scan.
        let scan_command = format!("{} {}", test_data.scan_predicate, test_value);
        reference_scanmem.write_line_stdin(scan_command.as_str()).unwrap();
        let reference_match_data: MatchData = reference_scanmem.read_match_data();

        for i in 0..test_data.thread_configs.len() {
            let test_scanmem = &mut test_scanmem_list[i];
            test_scanmem.write_line_stdin(scan_command.as_str()).unwrap();
            let test_match_data: MatchData = test_scanmem.read_match_data();
            // Validate.
            expect_eq_r!(test_result, reference_match_data.error, false, format!("nthreads {} failed", test_data.thread_configs[i]));
            expect_eq_r!(test_result, test_match_data.error, false, format!("nthreads {} failed", test_data.thread_configs[i]));
            expect_eq_r!(test_result, test_match_data.match_count, reference_match_data.match_count, format!("nthreads {} failed", test_data.thread_configs[i]));
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

pub const TEST_DATA_TYPES_FIXED_SIZE_FIXTURE_COUNT: usize = 9;

pub fn scenario_func_test_data_types_fixed_size(reference_scanmem_program: &str, test_scanmem_program: &str, synthetic_load_program: &str, _synthetic_load_random_seed: u64, fixture_index: usize, verbose: bool) -> TestResult {
    // How many threads to use.
    const THREAD_COUNT_ARRAY: [u32; 6] = [
        1, 2, 3, 11, 20, 32
    ];

    const SYNTHETIC_LOAD_SIZE0: usize = 0x1_000_000usize;
    const SYNTHETIC_LOAD_SIZE1: usize = SYNTHETIC_LOAD_SIZE0 / 2;

    let test_fixtures: [TestData; TEST_DATA_TYPES_FIXED_SIZE_FIXTURE_COUNT] = [
        TestData {
            scan_data_type: "number".into(),
            scan_predicate: "=".into(),
            test_values: vec![(SYNTHETIC_LOAD_SIZE0, "-123123123".into()),(SYNTHETIC_LOAD_SIZE1, "0".into())],
            thread_configs: THREAD_COUNT_ARRAY.to_vec()
        },
        TestData {
            scan_data_type: "int".into(),
            scan_predicate: "=".into(),
            test_values: vec![(SYNTHETIC_LOAD_SIZE0, "123123".into()),(SYNTHETIC_LOAD_SIZE1, "-123123123".into())],
            thread_configs: THREAD_COUNT_ARRAY.to_vec()
        },
        TestData {
            scan_data_type: "float".into(),
            scan_predicate: "=".into(),
            test_values: vec![(SYNTHETIC_LOAD_SIZE0, "0.123123123".into()),(SYNTHETIC_LOAD_SIZE1, "123.123123".into())],
            thread_configs: THREAD_COUNT_ARRAY.to_vec()
        },
        TestData {
            scan_data_type: "int8".into(),
            scan_predicate: "=".into(),
            test_values: vec![(SYNTHETIC_LOAD_SIZE0, "123".into()),(SYNTHETIC_LOAD_SIZE1, "1".into())],
            thread_configs: THREAD_COUNT_ARRAY.to_vec()
        },
        TestData {
            scan_data_type: "int16".into(),
            scan_predicate: "=".into(),
            test_values: vec![(SYNTHETIC_LOAD_SIZE0, "12312".into()),(SYNTHETIC_LOAD_SIZE1, "-10".into())],
            thread_configs: THREAD_COUNT_ARRAY.to_vec()
        },
        TestData {
            scan_data_type: "int32".into(),
            scan_predicate: "=".into(),
            test_values: vec![(SYNTHETIC_LOAD_SIZE0, "123123123".into()),(SYNTHETIC_LOAD_SIZE1, "1111".into())],
            thread_configs: THREAD_COUNT_ARRAY.to_vec()
        },
        TestData {
            scan_data_type: "int64".into(),
            scan_predicate: "=".into(),
            test_values: vec![(SYNTHETIC_LOAD_SIZE0, "123123123123123".into()),(SYNTHETIC_LOAD_SIZE1, "-100000000000".into())],
            thread_configs: THREAD_COUNT_ARRAY.to_vec()
        },
        TestData {
            scan_data_type: "float32".into(),
            scan_predicate: "=".into(),
            test_values: vec![(SYNTHETIC_LOAD_SIZE0, "-0.123123123123".into()),(SYNTHETIC_LOAD_SIZE1, "0.123123123123".into())],
            thread_configs: THREAD_COUNT_ARRAY.to_vec()
        },
        TestData {
            scan_data_type: "float64".into(),
            scan_predicate: "=".into(),
            test_values: vec![(SYNTHETIC_LOAD_SIZE0, "0.123123123123123123".into()),(SYNTHETIC_LOAD_SIZE1, "1.123123123123123123".into())],
            thread_configs: THREAD_COUNT_ARRAY.to_vec()
        },
    ];

    test_data_types(reference_scanmem_program, test_scanmem_program, synthetic_load_program, &test_fixtures[fixture_index], verbose)
}

pub const TEST_DATA_TYPES_STRING_FIXTURE_COUNT: usize = 2;

pub fn scenario_func_test_data_types_string(reference_scanmem_program: &str, test_scanmem_program: &str, synthetic_load_program: &str, _synthetic_load_random_seed: u64, fixture_index: usize, verbose: bool) -> TestResult {
    // How many threads to use.
    const THREAD_COUNT_ARRAY: [u32; 6] = [
        1, 2, 3, 11, 20, 32
    ];

    const SYNTHETIC_LOAD_SIZE0: usize = 0x1_000_000usize;
    const SYNTHETIC_LOAD_SIZE1: usize = SYNTHETIC_LOAD_SIZE0 / 2;

    let test_fixtures: [TestData; TEST_DATA_TYPES_STRING_FIXTURE_COUNT] = [
        TestData {
            scan_data_type: "string".into(),
            scan_predicate: "\"".into(),
            test_values: vec![(SYNTHETIC_LOAD_SIZE0, "This is a short string".into()),(SYNTHETIC_LOAD_SIZE1, "This string is longer than the first string".into())],
            thread_configs: THREAD_COUNT_ARRAY.to_vec()
        },
        TestData {
            scan_data_type: "string".into(),
            scan_predicate: "\"".into(),
            test_values: vec![(SYNTHETIC_LOAD_SIZE0, "abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz".into()),(SYNTHETIC_LOAD_SIZE1, "one".into())],
            thread_configs: THREAD_COUNT_ARRAY.to_vec()
        },
    ];

    test_data_types(reference_scanmem_program, test_scanmem_program, synthetic_load_program, &test_fixtures[fixture_index], verbose)
}

pub const TEST_DATA_TYPES_BYTEARRAY_FIXTURE_COUNT: usize = 4;

pub fn scenario_func_test_data_types_bytearray(reference_scanmem_program: &str, test_scanmem_program: &str, synthetic_load_program: &str, _synthetic_load_random_seed: u64, fixture_index: usize, verbose: bool) -> TestResult {
    // How many threads to use.
    const THREAD_COUNT_ARRAY: [u32; 6] = [
        1, 2, 3, 11, 20, 32
    ];

    const SYNTHETIC_LOAD_SIZE0: usize = 0x1_000_000usize;
    const SYNTHETIC_LOAD_SIZE1: usize = SYNTHETIC_LOAD_SIZE0 / 2;

    let test_fixtures: [TestData; TEST_DATA_TYPES_BYTEARRAY_FIXTURE_COUNT] = [
        TestData {
            scan_data_type: "bytearray".into(),
            scan_predicate: "".into(),
            test_values: vec![(SYNTHETIC_LOAD_SIZE0, "01 02 03 04 05 06".into()),(SYNTHETIC_LOAD_SIZE1, "01 02 03 04 05 06".into())],
            thread_configs: THREAD_COUNT_ARRAY.to_vec()
        },
        TestData {
            scan_data_type: "bytearray".into(),
            scan_predicate: "".into(),
            test_values: vec![(SYNTHETIC_LOAD_SIZE0, "??".into()),(SYNTHETIC_LOAD_SIZE1, "??".into())],
            thread_configs: THREAD_COUNT_ARRAY.to_vec()
        },
        TestData {
            scan_data_type: "bytearray".into(),
            scan_predicate: "".into(),
            test_values: vec![(SYNTHETIC_LOAD_SIZE0, "FF ?? EE ?? 02 01".into()),(SYNTHETIC_LOAD_SIZE1, "??".into())],
            thread_configs: THREAD_COUNT_ARRAY.to_vec()
        },
        TestData {
            scan_data_type: "bytearray".into(),
            scan_predicate: "".into(),
            test_values: vec![(SYNTHETIC_LOAD_SIZE0, "FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF".into()),(SYNTHETIC_LOAD_SIZE1, "FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF".into())],
            thread_configs: THREAD_COUNT_ARRAY.to_vec()
        },
    ];

    test_data_types(reference_scanmem_program, test_scanmem_program, synthetic_load_program, &test_fixtures[fixture_index], verbose)
}
