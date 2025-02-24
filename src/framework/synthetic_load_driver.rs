
use std::process::{Child, Command, ExitStatus, Stdio};

use crate::utils::*;

pub static SYNTHETIC_LOAD_NAME: &str = "synthetic_load";

pub struct SyntheticLoadDriver {
    child_process: Child,
    verbose: bool,
}

impl SyntheticLoadDriver {
    pub fn create(synthetic_load_program: &str, verbose: bool) -> std::io::Result<SyntheticLoadDriver> {
        
        let process = SyntheticLoadDriver {
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