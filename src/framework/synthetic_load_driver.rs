
use std::process::{Child, Command, ExitStatus, Stdio};

use crate::utils::*;

use nix::sys::wait::WaitStatus;
use nix::unistd::Pid;
use nix::sys;

pub static SYNTHETIC_LOAD_NAME: &str = "synthetic_load";

pub struct SyntheticLoadDriver {
    child_process: Child,
    verbose: bool,
}

impl Drop for SyntheticLoadDriver {
    fn drop(&mut self) {
        if self.verbose {
            println!("synthetic_load child process done, pid: {}", self.get_pid());
        }
    }
}

impl SyntheticLoadDriver {
    pub fn create(synthetic_load_program: &str, verbose: bool) -> std::io::Result<SyntheticLoadDriver> {
        
        let process = SyntheticLoadDriver {
            child_process: Command::new(synthetic_load_program).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn()?,
            verbose
        };

        if process.verbose {
            println!("Starting synthetic_load child process, pid = {}", process.child_process.id());
        }

        Ok(process)
    }

    fn send_signal(pid: Pid, signal: sys::signal::Signal) -> std::io::Result<()> {
        sys::signal::kill(pid, signal).map_err(|_e| {
            println!("Error! kill failed!");
            std::io::ErrorKind::Other
        })?;
        Ok(())
    }
    
    pub fn send_sigstop(&self) -> std::io::Result<()> {
        let pid: Pid = Pid::from_raw(self.child_process.id().try_into().unwrap());
        SyntheticLoadDriver::send_signal(pid, sys::signal::SIGSTOP)?;
        let res = sys::wait::waitpid( pid,Some(sys::wait::WaitPidFlag::WSTOPPED)).map_err(|_e|  {
            println!("Error! waidpid failed!");
            std::io::ErrorKind::Other
        })?;
        
        match res {
            WaitStatus::Stopped(p, sig) => {
                if self.verbose {
                    println!("Stopped synthetic_load child process, pid = {}, sig = {}", p, sig);
                }
            },
            _status => {
                println!("Error! unexpected signal received!");
                return Err(std::io::ErrorKind::Other.into());
            }
        }
        Ok(())
    }

    pub fn send_sigcont(&self) -> std::io::Result<()> {
        let pid: Pid = Pid::from_raw(self.child_process.id().try_into().unwrap());
        SyntheticLoadDriver::send_signal(pid, sys::signal::SIGCONT)?;
        let res = sys::wait::waitpid( pid,Some(sys::wait::WaitPidFlag::WCONTINUED)).map_err(|_e|  {
            println!("Error! waidpid failed!");
            std::io::ErrorKind::Other
        })?;
        
        match res {
            WaitStatus::Continued(p) => {
                if self.verbose {
                    println!("Continued synthetic_load child process, pid = {}", p);
                }
            },
            _status => {
                println!("Error! unexpected signal received!");
                return Err(std::io::ErrorKind::Other.into());
            }
        }
        Ok(())
    }

    /// Exit child process, will drain stdout.
    pub fn command_exit(&mut self) -> std::io::Result<()> {
        self.write_line_stdin("exit".to_string().as_str())?;
        self.drain_stdout()
    }

    /// Send command and wait for "Done" message.
    pub fn command_set_memory_size(&mut self, size: usize) -> std::io::Result<()> {
        self.write_line_stdin(format!("set-memory-size {}", size).as_str())?;
        self.read_until_line_stdout("Done")
    }

    /// Send command and wait for "Done" message.
    pub fn command_fill(&mut self, value: u8) -> std::io::Result<()> {
        self.write_line_stdin(format!("fill {}", value).as_str())?;
        self.read_until_line_stdout("Done")
    }

    /// Send command and wait for "Done" message.
    pub fn command_fill_random(&mut self, seed: u64) -> std::io::Result<()> {
        self.write_line_stdin(format!("fill-random {}", seed).as_str())?;
        self.read_until_line_stdout("Done")
    }

    /// Send command and wait for "Done" message.
    pub fn command_set_address(&mut self, address: usize, value: u8) -> std::io::Result<()> {
        self.write_line_stdin(format!("set-address {} {}", address, value).as_str())?;
        self.read_until_line_stdout("Done")
    }

    pub fn get_pid(&self) -> u32 {
        self.child_process.id()
    }

    pub fn wait(&mut self) -> std::io::Result<ExitStatus> {
        self.child_process.wait()
    }

    fn read_line_stdout(&mut self) -> std::io::Result<String> {
        assert!(self.child_process.stdout.is_some());
        
        let pid = self.child_process.id();
        return internal_read_line(self.child_process.stdout.as_mut().unwrap(), pid,self.verbose, "stdout");
    }

    /// Read from stdout until exact line is present.
    /// Discards read lines.
    fn read_until_line_stdout(&mut self, condition_line: &str) -> std::io::Result<()> {
        loop {
            let buf = self.read_line_stdout()?;
            if buf.eq(format!("{}\n", condition_line).as_str()) {
                return Ok(());
            }
        }
    }

    fn drain_stdout(&mut self) -> std::io::Result<()> {
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

    fn write_line_stdin(&mut self, line: &str) -> std::io::Result<()> {
        assert!(self.child_process.stdin.is_some());
        
        let pid = self.child_process.id();
        return internal_write_line(self.child_process.stdin.as_mut().unwrap(), line, pid, self.verbose, "stdin");
    }
}