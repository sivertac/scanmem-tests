
use std::{io::{BufRead, BufReader, BufWriter, Read, Write}, process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus, Stdio}};

pub static SYNTHETIC_LOAD_NAME: &str = "synthetic_load";

pub fn read_line_stream<S: BufRead>(stream: &mut S, pid: u32, echo: bool, name: &str) -> Result<String, String> {
    let mut buf = String::new();
    stream.read_line(&mut buf).map_err(|e|e.to_string())?;
    if echo && !buf.is_empty() {
        print!("pid {} {}: {}", pid, name, buf);
    }
    return Ok(buf)
}

/// Read 1 line from stdout and return it, blocking.
pub fn read_line_stdout(stdout: & mut BufReader<ChildStdout>, pid: u32, echo: bool) -> Result<String, String> {
    return read_line_stream(stdout, pid, echo, "stdout");
}

/// Read 1 line from stderr and return it, blocking.
pub fn read_line_stderr(stderr: & mut BufReader<ChildStderr>, pid: u32, echo: bool) -> Result<String, String> {
    return read_line_stream(stderr, pid, echo, "stderr");
}

/// Read from stdout until exact line is present.
/// Discards read lines.
pub fn read_until_line_stdout(stdout: & mut BufReader<ChildStdout>, condition_line: &str, pid: u32, echo: bool) -> Result<(), String> {
    loop {
        let buf = read_line_stdout(stdout, pid, echo)?;
        if buf.eq(format!("{}\n", condition_line).as_str()) {
            return Ok(())
        }
    }
}

// Write line to stream, blocking.
pub fn write_line(stdin: &mut BufWriter<ChildStdin>, line: &str, pid: u32, echo: bool) -> Result<(), String> {
    let out = format!("{}\n", line);
    if echo {
        print!("pid {} stdin: {}", pid, out);
    }
    stdin.write_all(out.as_bytes()).map_err(|e|e.to_string())?;
    stdin.flush().map_err(|e|e.to_string())?;
    return Ok(())
}

// Read whats left in the output pipe.
pub fn drain_stream<S: BufRead>(stream: &mut S, pid: u32, echo: bool, name: &str) -> Result<(), String> {
    loop {
        let buf = read_line_stream(stream, pid, echo, name)?;                
        if buf.len() == 0 {
            break;
        }
    }
    Ok(())
}

pub fn drain_stdout(stdout: & mut BufReader<ChildStdout>, pid: u32, echo: bool) -> Result<(), String> {
    return drain_stream(stdout, pid, echo, "stdout");
}

pub fn drain_stderr(stderr: & mut BufReader<ChildStderr>, pid: u32, echo: bool) -> Result<(), String> {
    return drain_stream(stderr, pid, echo, "stderr");
}

pub fn parse_scanmem_commands(input: &str) -> Vec<&str> {

    let ret: Vec<&str> = input.split(';').collect();

    // check if last command is 'exit'
    if let Some(last) = ret.last() {
        if !last.trim_ascii().eq("exit") {
            println!("Warning: scanmem commands does not exit with 'exit'!.");
        }
    }
    return ret;
}

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

pub struct SyntheticLoadProcess {
    child_process: Child,
    verbose: bool,
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

impl SyntheticLoadProcess {
    pub fn create(synthetic_load_program: &str, verbose: bool) -> std::io::Result<SyntheticLoadProcess> {
        
        let process = SyntheticLoadProcess {
            child_process: match Command::new(synthetic_load_program).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn() {
                Ok(c) => c,
                Err(e) => {
                    return Err(e)    
                }
            },
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
