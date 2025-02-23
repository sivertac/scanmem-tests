
use std::{io::{BufRead, BufReader, BufWriter, Read, Write}, process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus, Stdio}, sync::{Arc, Mutex, Condvar}, thread::JoinHandle};

pub static SYNTHETIC_LOAD_NAME: &str = "synthetic_load";

pub fn compute_median<I>(values: I) -> f64 where I: Iterator<Item = f64>, {
    let mut data: Vec<f64> = values.collect();
    data.sort_by(|a,b|a.total_cmp(b));
    return data[data.len() / 2];
}

pub fn compute_standard_deviation<I>(values: I, mean: f64) -> f64 where I: Iterator<Item = f64>, {
    let data: Vec<f64> = values.collect();
    let len = data.len();
    let sum = data.into_iter().reduce(|acc: f64, e: f64| acc + (e - mean)).unwrap();
    return f64::sqrt(1.0f64 / len as f64 * sum.powi(2));
}

fn internal_read_line<S: std::io::Read>(stream: &mut S, pid: u32, echo: bool, name: &str) -> std::io::Result<String> {
    let mut ret = String::new();

    // Read one char at a time
    let mut buf: [u8; 1] = [0; 1]; 
    loop {
        stream.read_exact(&mut buf)?;
        ret.push(buf[0] as char);
        if buf[0] as char == '\n' {
            break
        }
    }
    
    if echo && !buf.is_empty() {
        print!("pid {} {}: {}", pid, name, ret);
    }

    return Ok(ret)
}

// Read what's left in the output pipe.
fn internal_drain_stream<S: std::io::Read>(stream: &mut S, pid: u32, echo: bool, name: &str) -> std::io::Result<()> {
    loop {
        match internal_read_line(stream, pid, echo, name) {
            Ok(buf) => {
                if buf.len() == 0 {
                    return Ok(());
                }
            },
            Err(e) => {
                if e.kind() == std::io::ErrorKind::UnexpectedEof {
                    // Hit EOF, exit.
                    return Ok(());
                }
                return Err(e);
            }
        }                
    }
}

// Write line to stream, blocking.
// Appends '/n' to end of 'line' string.
pub fn internal_write_line<S: std::io::Write>(stream: &mut S, line: &str, pid: u32, echo: bool, name: &str) -> std::io::Result<()> {
    let out = format!("{}\n", line);
    if echo {
        print!("pid {} {}: {}", pid, name, out);
    }
    stream.write_all(out.as_bytes())?;
    stream.flush()?;

    return Ok(())
}

pub struct SyntheticLoadProcess {
    child_process: Child,
    verbose: bool,
}

impl SyntheticLoadProcess {
    pub fn create(synthetic_load_program: &str, verbose: bool) -> std::io::Result<SyntheticLoadProcess> {
        
        let process = SyntheticLoadProcess {
            child_process: Command::new(synthetic_load_program).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn()?,
            verbose: verbose
        };

        if process.verbose {
            println!("Starting synthetic_load child process, pid = {}", process.child_process.id());
        }

        return Ok(process);
    }

    pub fn get_pid(&self) -> u32 {
        return self.child_process.id();
    }

    pub fn wait(&mut self) -> std::io::Result<ExitStatus> {
        return self.child_process.wait();
    }

    pub fn read_line_stdout(&mut self) -> std::io::Result<String> {
        assert!(self.child_process.stdout.is_some());
        
        let pid = self.child_process.id();
        return internal_read_line(self.child_process.stdout.as_mut().unwrap(), pid,self.verbose, "stdout");
    }

    /// Read from stdout until exact line is present.
    /// Discards read lines.
    pub fn read_until_line_stdout(&mut self, condition_line: &str) -> std::io::Result<()> {
        loop {
            let buf = self.read_line_stdout()?;
            if buf.eq(format!("{}\n", condition_line).as_str()) {
                return Ok(());
            }
        }
    }

    pub fn drain_stdout(&mut self) -> std::io::Result<()> {
        assert!(self.child_process.stdout.is_some());
        
        let pid = self.child_process.id();
        return internal_drain_stream(self.child_process.stdout.as_mut().unwrap(), pid, self.verbose, "stdout");
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

#[derive(Debug)]
pub struct MatchData {
    pub error: bool,
    pub match_count: u64,
}

pub struct ScanmemProcess {
    child_process: Child,
    verbose: bool,

    stdout_thread: Option<JoinHandle<()>>,
    stderr_thread: Option<JoinHandle<()>>,

    match_data: Arc<(Mutex<Option<MatchData>>, Condvar)>,
}

impl ScanmemProcess {
    pub fn create(scanmem_program: &str, target_process_pid: u32, nthreads: i32, verbose: bool) -> std::io::Result<ScanmemProcess> {
        
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
        let mut process = ScanmemProcess {
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


