
use crate::framework::utils::TestResult;

mod test_search_regions;
mod test_check_matches;
mod test_snapshot;
mod test_data_types;

pub type TestScenarioFunc = fn(reference_scanmem_program: &str, test_scanmem_program: &str, synthetic_load_program: &str, synthetic_load_random_seed: u64, fixture_index: usize, verbose: bool) -> TestResult;


pub struct TestScenario {
    pub name: String,
    pub description: String,
    pub perform_test_scenario_func: TestScenarioFunc,
    pub fixture_count: usize,
}

pub fn get_test_list() -> Vec<TestScenario> {
    vec![
        TestScenario{
            name: "SearchRegions".into(),
            description: "Fill target process with random bytes, then call scanmem with \"= 1; q;\". Compare matches found to reference.".into(),
            perform_test_scenario_func: test_search_regions::scenario_func_test_search_regions,
            fixture_count: 6,
        },
        TestScenario{
            name: "CheckMatches".into(),
            description: "Fill target process with 1s, and call scanmem with \"= 1\". Then modify target process to contain 2s, and call scanmem with \"= 2\". Compare matches found to reference.".into(),
            perform_test_scenario_func: test_check_matches::scenario_func_test_check_matches,
            fixture_count: 6,
        },
        TestScenario{
            name: "Snapshot".into(),
            description: "Test snapshot feature of scanmem. Take snapshot of target process and compare to reference.".into(),
            perform_test_scenario_func: test_snapshot::scenario_func_test_snapshot,
            fixture_count: 1,
        },
        TestScenario{
            name: "DataTypesFixedSize".into(),
            description: "Test all supported fixed size data types.".into(),
            perform_test_scenario_func: test_data_types::scenario_func_test_data_types_fixed_size,
            fixture_count: test_data_types::TEST_DATA_TYPES_FIXED_SIZE_FIXTURE_COUNT,
        },
        TestScenario{
            name: "DataTypesString".into(),
            description: "Test string data type.".into(),
            perform_test_scenario_func: test_data_types::scenario_func_test_data_types_string,
            fixture_count: test_data_types::TEST_DATA_TYPES_STRING_FIXTURE_COUNT,
        },
    ]
}