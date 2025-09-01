use std::{process::{Child, Command, ExitStatus, Stdio}, sync::{Arc, Mutex, Condvar}, thread::JoinHandle};
use regex::Regex;

use super::utils::*;

#[derive(Debug)]
pub struct MatchData {
    pub error: bool,
    pub match_count: u64,
}

#[derive(Debug)]
struct ScanmemVersions {
    scanmem_version: (i32, i32),
    libscanmem_version: (i32, i32)
}

fn read_scanmem_versions(scanmem_program: &str, verbose: bool) -> std::io::Result<ScanmemVersions> {
    let mut scanmem_version= (0, 0);
    let mut libscanmem_version= (0, 0);
    
    let dummy_process = Command::new(scanmem_program).arg("--version").stderr(Stdio::piped()).spawn()?;
    let output = dummy_process.wait_with_output()?;
    let output_string = String::from_utf8(output.stderr).unwrap();

    // Parse out versions.
    let scanmem_regex = Regex::new(r"^scanmem version (?<major>[0-9]+)\.(?<minor>[0-9]+)").unwrap();
    let libscanmem_regex = Regex::new(r"^libscanmem version (?<major>[0-9]+)\.(?<minor>[0-9]+)").unwrap();
    for line in output_string.lines() {
        if let Some(captures) = scanmem_regex.captures(line) {
            let major: i32 = captures.name("major").unwrap().as_str().parse().unwrap();
            let minor: i32 = captures.name("minor").unwrap().as_str().parse().unwrap();
            scanmem_version = (major, minor);
        }
        if let Some(captures) = libscanmem_regex.captures(line) {
            let major: i32 = captures.name("major").unwrap().as_str().parse().unwrap();
            let minor: i32 = captures.name("minor").unwrap().as_str().parse().unwrap();
            libscanmem_version = (major, minor);
        }
    }

    let versions = ScanmemVersions{scanmem_version, libscanmem_version};
    if verbose {
        println!("scanmem versions: {:?}", versions);
    }
    
    Ok(versions)
}

fn check_if_scanmem_program_supports_multithreading(scanmem_program: &str, verbose: bool) -> std::io::Result<bool> {
    // Get scanmem versions.

    let versions = read_scanmem_versions(scanmem_program, verbose)?;

    let scanmem_supports_multithreading = versions.scanmem_version.0 >= 0 && versions.scanmem_version.1 >= 18 && versions.libscanmem_version.0 >= 0 && versions.libscanmem_version.1 >= 18;
    if scanmem_supports_multithreading && verbose {
        println!("Scanmem ({}) supports multithreading.", scanmem_program);
    }

    Ok(scanmem_supports_multithreading)
}

pub struct ScanmemDriver {
    child_process: Child,
    verbose: bool,

    stdout_thread: Option<JoinHandle<()>>,
    stderr_thread: Option<JoinHandle<()>>,

    match_data: Arc<(Mutex<Option<MatchData>>, Condvar)>,
}

impl Drop for ScanmemDriver {
    fn drop(&mut self) {
        if self.verbose {
            println!("scanmem child process done, pid: {}", self.get_pid());
        }
    }
}

impl ScanmemDriver {
    /// Setting nthreads = 0 is the same as autodetecting thread count.
    pub fn create(scanmem_program: &str, target_process_pid: u32, nthreads: u32, verbose: bool) -> std::io::Result<ScanmemDriver> {
        // Compile args.
        let mut args: String = format!("--pid={}", target_process_pid);
        let scanmem_supports_multithreading = check_if_scanmem_program_supports_multithreading(scanmem_program, verbose)?;
        if scanmem_supports_multithreading {
            args.push_str(format!(" --jobs={}", nthreads).as_str());
        }
        let args_vec: Vec<&str> = args.split_ascii_whitespace().collect();

        // Create scanmem child process.
        let mut process = ScanmemDriver {
            child_process: Command::new(scanmem_program).args(args_vec).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?,
            verbose,
            stdout_thread: None,
            stderr_thread: None,
            match_data: Arc::new((Mutex::new(None), Condvar::new())),
        };

        process.start_stdout_thread()?;
        process.start_stderr_thread()?;

        if process.verbose {
            println!("Starting scanmem child process, pid = {}", process.child_process.id());
        }

        Ok(process)
    }

    fn start_stdout_thread(&mut self) -> std::io::Result<()> {
        let mut stdout_stream = self.child_process.stdout.take().unwrap();
        let verbose = self.verbose;
        let pid = self.child_process.id();
        self.stdout_thread = Some(std::thread::spawn(move || {
            internal_drain_stream(&mut stdout_stream, pid, verbose, "stdout").unwrap();
        }));

        Ok(())
    }

    fn start_stderr_thread(&mut self) -> std::io::Result<()> {
        let mut stderr_stream = self.child_process.stderr.take().unwrap();
        let verbose = self.verbose;
        let pid = self.child_process.id();
        let match_data = self.match_data.clone();
        self.stderr_thread = Some(std::thread::spawn(move || {
            loop {
                let res = internal_read_line(&mut stderr_stream, pid, verbose, "stderr");

                let buf;
                if let Err(e) = res {
                    if e.kind() == std::io::ErrorKind::UnexpectedEof {
                        break;
                    }
                    else if e.kind() == std::io::ErrorKind::Interrupted {
                        continue;
                    }
                    else {
                        assert!(false);
                        break
                    }
                }
                else {
                    buf = res.ok().unwrap();
                }

                if buf.is_empty() {
                    break;
                }

                const MATCH_COUNT_PREFIX: &str = "info: we currently have ";
                const MATCH_COUNT_SUFFIX: &str = " matches.\n";
                if let Some(sub_str) = buf.strip_prefix(MATCH_COUNT_PREFIX) {
                    // Store matches
                    let match_count_string = sub_str.strip_suffix(MATCH_COUNT_SUFFIX).unwrap();

                    let mut guard = match_data.0.lock().unwrap();
                    
                    *guard = Some(MatchData{
                        error: false,
                        match_count: match_count_string.parse().unwrap(),
                    });
                    match_data.1.notify_one();
                    
                }
                else if buf.contains("Operation not permitted") {
                    // Check if we have a permission error when running scanmem.

                    let mut guard = match_data.0.lock().unwrap();
                    
                    *guard = Some(MatchData{
                        error: true,
                        match_count: 0,
                    });
                    match_data.1.notify_one();
                }
            }
        }));

        Ok(())
    }

    // Blocks until match data is read from scanmem process, a scan must have been sent to scanmem before this function is called, this will block until the match is read, if not this will block forever.
    pub fn read_match_data(&mut self) -> MatchData {
        let mut guard = self.match_data.0.lock().unwrap();

        // Block until read thread has read the match count from scanmem.
        while guard.is_none() {
            guard = self.match_data.1.wait(guard).unwrap();
        }

        // Read and reset.
        

        guard.take().unwrap()
    }

    pub fn get_pid(&self) -> u32 {
        self.child_process.id()
    }

    pub fn wait(&mut self) -> std::io::Result<ExitStatus> {
        self.child_process.wait()
    }

    //pub fn write_all_stdin(&self, data: &str) -> std::io::Result<()> {
    //    assert!(self.child_process.stdin.is_some());
    //
    //    self.child_process.stdin.as_ref().unwrap().write_all(data.as_bytes())?;
    //    self.child_process.stdin.as_ref().unwrap().flush()?;
    //
    //    return Ok(());
    //}

    pub fn write_line_stdin(&mut self, line: &str) -> std::io::Result<()> {
        assert!(self.child_process.stdin.is_some());
        
        let pid = self.child_process.id();
        return internal_write_line(self.child_process.stdin.as_mut().unwrap(), line, pid, self.verbose, "stdin");
    }
}

pub fn scanmem_data_type_to_bytes(data_type: &str, value: &str) -> Vec<u8> {
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

pub fn bytearray_to_scanmem_input(bytearray: &[u8]) -> String {
    let mut ret = String::new();

    for v in bytearray {
        ret.push_str(format!("{:02X} ", v).as_str());
    }

    ret
}

pub fn scanmem_bytearray_to_bytes(input: &str) -> Result<Vec<u8>, std::num::ParseIntError> {
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
