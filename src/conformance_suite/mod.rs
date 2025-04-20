
use crate::framework::utils::TestResult;

pub mod test_search_regions;
pub mod test_check_matches;

pub type TestScenarioFunc = fn(reference_scanmem_program: &str, test_scanmem_program: &str, synthetic_load_program: &str, synthetic_load_random_seed: u64, fixture_index: usize, verbose: bool) -> TestResult;


pub struct TestScenario {
    pub name: String,
    pub description: String,
    pub perform_benchmark_scenario_func: TestScenarioFunc,
    pub fixtures_count: usize,
}

pub fn get_test_list() -> Vec<TestScenario> {
    vec![
        TestScenario{
            name: "SearchRegions".into(),
            description: "Fill target process with random bytes, then call scanmem with \"= 1; q;\". Compare matches found to reference.".into(),
            perform_benchmark_scenario_func: test_search_regions::scenario_func_test_search_regions,
            fixtures_count: 6,
        },
        TestScenario{
            name: "CheckMatches".into(),
            description: "Fill target process with 1s, and call scanmem with \"= 1\". Then modify target process to contain 2s, and call scanmem with \"= 2\". Compare matches found to reference.".into(),
            perform_benchmark_scenario_func: test_check_matches::scenario_func_test_check_matches,
            fixtures_count: 6,
        },
    ]
}