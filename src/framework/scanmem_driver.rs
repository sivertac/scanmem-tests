use std::{process::{Child, Command, ExitStatus, Stdio}, sync::{Arc, Mutex, Condvar}, thread::JoinHandle};

use crate::utils::*;

#[derive(Debug)]
pub struct MatchData {
    pub error: bool,
    pub match_count: u64,
}

pub struct ScanmemDriver {
    child_process: Child,
    verbose: bool,

    stdout_thread: Option<JoinHandle<()>>,
    stderr_thread: Option<JoinHandle<()>>,

    match_data: Arc<(Mutex<Option<MatchData>>, Condvar)>,
}

impl ScanmemDriver {
    pub fn create(scanmem_program: &str, target_process_pid: u32, nthreads: i32, verbose: bool) -> std::io::Result<ScanmemDriver> {
        
        // Compile args.
        let args: String;
        if nthreads == -1 {
            args = format!("--pid={}", target_process_pid);
        }
        else {
            args = format!("--pid={} --jobs={}", target_process_pid, nthreads);
        }
        let args_vec: Vec<&str> = args.split_ascii_whitespace().collect();

        // Create scanmem child process.
        let mut process = ScanmemDriver {
            child_process: Command::new(scanmem_program).args(args_vec).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?,
            verbose: verbose,
            stdout_thread: None,
            stderr_thread: None,
            match_data: Arc::new((Mutex::new(None.into()), Condvar::new())),
        };

        process.start_stdout_thread()?;
        process.start_stderr_thread()?;

        if process.verbose {
            println!("Starting scanmem child process, pid = {}", process.child_process.id());
        }

        return Ok(process);
    }

    fn start_stdout_thread(&mut self) -> std::io::Result<()> {
        let mut stdout_stream = self.child_process.stdout.take().unwrap();
        let verbose = self.verbose;
        let pid = self.child_process.id();
        self.stdout_thread = Some(std::thread::spawn(move || {
            internal_drain_stream(&mut stdout_stream, pid, verbose, "stdout").unwrap();
        }));

        return Ok(());
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

                if buf.len() == 0 {
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

        return Ok(());
    }

    // Blocks until match data is read from scanmem process, a scan must have been sent to scanmem before this function is called, this will block until the match is read, if not this will block forever.
    pub fn read_match_data(&mut self) -> MatchData {
        let mut guard = self.match_data.0.lock().unwrap();

        // Block until read thread has read the match count from scanmem.
        while guard.is_none() {
            guard = self.match_data.1.wait(guard).unwrap();
        }

        // Read and reset.
        let match_data = guard.take().unwrap();

        return match_data;
    }

    pub fn get_pid(&self) -> u32 {
        return self.child_process.id();
    }

    pub fn wait(&mut self) -> std::io::Result<ExitStatus> {
        return self.child_process.wait();
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
