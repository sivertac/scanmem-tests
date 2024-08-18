
use std::{borrow::Borrow, io::{BufRead, BufReader, BufWriter, Read, Write}, os::unix::thread, process::{Child, ChildStdin, ChildStdout, Command, ExitCode, Stdio}, thread::sleep, time::{Duration, SystemTime, SystemTimeError}};
use clap::{Parser, Subcommand};
use clap_num::maybe_hex;

static SYNTHETIC_LOAD_NAME: &str = "synthetic_load";

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    
    scanmem_program: String

}

#[derive(Default, Debug)]
struct BenchmarkReport {
    setup_time: Duration,
    scanmem_time: Duration,
    total_time: Duration
}

struct ChildProcess {
    child_process: Child,
    stdin: BufWriter<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    echo: bool
}

impl ChildProcess {
    fn new(command: &str, args: &str, echo: bool) -> Result<ChildProcess, String> {
        let args_vec: Vec<&str> = args.split_ascii_whitespace().collect();
        let mut c = match Command::new(command).args(args_vec).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn() {
            Ok(c) => c,
            Err(e) => {
                return Err(e.to_string())    
            }
        };
        let stdin = BufWriter::new(c.stdin.take().unwrap());
        let stdout = BufReader::new(c.stdout.take().unwrap());
        return Ok(ChildProcess{child_process: c, stdin: stdin, stdout: stdout, echo: echo})
    }

    fn read_until_line(&mut self, condition_line: &str) -> Result<(), String> {
        loop {
            let mut buf = String::new();
            self.stdout.read_line(&mut buf).map_err(|e|e.to_string())?;
            if self.echo {
                print!("{}", buf);
            }
            if buf.eq(format!("{}\n", condition_line).as_str()) {
                return Ok(())
            }
        }
    }


    fn write_line(&mut self, line: &str) -> Result<(), String> {
        let out = format!("{}\n", line);
        if self.echo {
            print!("{}", out);
        }
        self.stdin.write_all(out.as_bytes()).map_err(|e|e.to_string())?;
        self.stdin.flush().map_err(|e|e.to_string())?;
        return Ok(())
    }
}

fn print_stdout_line(stdout: &mut ChildStdout) {
    let mut reader = BufReader::new(stdout);
    let mut buf = String::new();
    reader.read_line(&mut buf);
    println!("{}", buf);
}

fn perform_benchmark(scanmem_program: &str, scanmem_commands: Vec<&str>, synthetic_load_program: &str, synthetic_load_size: u64, synthetic_load_random_seed: u64) -> Result<BenchmarkReport, String> {

    let mut report = BenchmarkReport::default();

    let total_start_time = SystemTime::now();

    // Create synthetic_load child process and init
    println!("Starting synthetic_load child process...");
    let mut synthetic_load = ChildProcess::new(synthetic_load_program, "",true)?;
    println!("Child pid: {}", synthetic_load.child_process.id());
    synthetic_load.write_line(format!("set-memory-size {}", synthetic_load_size).as_str())?;
    synthetic_load.read_until_line("Done")?;
    synthetic_load.write_line(format!("fill-random {}", synthetic_load_random_seed).as_str())?;
    synthetic_load.read_until_line("Done")?;

    
    report.setup_time = SystemTime::now().duration_since(total_start_time).map_err(|e|e.to_string())?;
    
    // Create scanmem child process
    println!("Starting scanmem child process...");
    let scanmem_start_time = SystemTime::now();
    let mut scanmem = ChildProcess::new(scanmem_program, format!("--pid={}", synthetic_load.child_process.id()).as_str(), true)?;
    //sleep(Duration::from_secs(1));
    //scanmem.stdout.
    for command in scanmem_commands {
        scanmem.write_line(command)?;
    }
    
    // Cleanup
    //synthetic_load.write_line("exit")?;
    scanmem.child_process.wait().unwrap();

    synthetic_load.write_line(format!("exit").as_str())?;
    synthetic_load.child_process.wait().unwrap();

    report.scanmem_time = SystemTime::now().duration_since(scanmem_start_time).map_err(|e|e.to_string())?;
    report.total_time = SystemTime::now().duration_since(total_start_time).map_err(|e|e.to_string())?;

    return Ok(report)
}

fn main() -> ExitCode {

    let cli = Cli::parse();

    let synthetic_load_path = std::env::current_exe().unwrap().parent().unwrap().to_path_buf().join(SYNTHETIC_LOAD_NAME);
    
    let report_result = perform_benchmark(&cli.scanmem_program, vec!["= 1", "= 1", "exit"], synthetic_load_path.to_str().unwrap(), 0x10000000, 0x1);

    println!("{:?}", report_result);

    return ExitCode::SUCCESS
}
