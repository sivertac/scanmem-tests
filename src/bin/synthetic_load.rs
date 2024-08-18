
use clap::{Parser, Subcommand};
use clap_num::maybe_hex;
use rustyline::error::ReadlineError;
use rustyline::{DefaultEditor, Result};
use rand::{Rng, SeedableRng};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, PartialEq, Eq, Debug)]
enum Commands {
    #[clap(alias = "q")]
    Exit,
    SetMemorySize {
        #[clap(value_parser=maybe_hex::<usize>)]
        new_memory_size: usize
    },
    Fill {
        #[clap(value_parser=maybe_hex::<u8>)]
        value: u8
    },
    FillRandom {
        #[clap(value_parser=maybe_hex::<u64>)]
        seed: u64
    },
    SetAddress {
        #[clap(value_parser=maybe_hex::<usize>)]
        address: usize,
        #[clap(value_parser=maybe_hex::<u8>)]
        value: u8
    },
    Info
}

#[derive(Debug)]
struct State {
    memory: Vec<u8>
}

static PROMPT: &str = "synthetic-load> ";

fn prepare_input_line(line: &String) -> Vec<String> {
    let mut v: Vec<String> = line.split_ascii_whitespace().map(str::to_string).collect();

    // append PROMPT to front so clap will work
    v.insert(0, PROMPT.to_string());
    return v;
}

fn set_memory_size(state: &mut State, new_size: usize) {
    state.memory.resize(new_size, 0x0);
    state.memory.shrink_to_fit();
}

fn fill_memory(state: &mut State, value: u8) {
    state.memory.fill(value);
}

fn fill_memory_random(state: &mut State, seed: u64) {
    let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(seed);
    let distr = rand::distributions::Uniform::new(u8::MIN, u8::MAX);
    state.memory.fill_with(||rng.sample(distr));
}

fn set_address(state: &mut State, address: usize, value: u8) {
    if state.memory.is_empty() {
        println!("memory empty");
        return;    
    }

    let memory_base_ptr = state.memory.as_ptr() as usize;
    let memory_range = memory_base_ptr..memory_base_ptr + state.memory.len();
    if !memory_range.contains(&address) {
        println!("address not in range");
        return;
    }

    let index = address - memory_base_ptr;
    state.memory[index] = value;
}

fn print_info(state: &State) {
    println!("memory size: {:#x}", state.memory.len());
    println!("memory start: {:#x}", state.memory.as_ptr() as usize);
    println!("memory end: {:#x}", (state.memory.as_ptr() as usize) + state.memory.len())
}

fn perform_command(state: &mut State, cli: Cli) {
    match cli.command {
        Commands::SetMemorySize { new_memory_size } => set_memory_size(state, new_memory_size),
        Commands::Info => print_info(state),
        Commands::Fill { value } => fill_memory(state, value),
        Commands::FillRandom { seed } => fill_memory_random(state, seed),
        Commands::SetAddress { address, value } => set_address(state, address, value),
        _ => {
            
        }
    }
}

fn main() -> Result<()> {
    // `()` can be used when no completer is required
    let mut rl = DefaultEditor::new()?;

    let mut state = State{ memory: vec![] };

    loop {
        let readline = rl.readline(PROMPT);
        match readline {
            Ok(line) => {
                let _ = rl.add_history_entry(line.as_str());
                let res = Cli::try_parse_from(prepare_input_line(&line));
                match res {
                    Ok(cli) => {
                        if cli.command == Commands::Exit {
                            break;
                        }
                        perform_command(&mut state, cli);
                        println!("Done");
                    }
                    Err(e) => {
                        println!("{}", e);
                        continue;
                    }
                }        
            },
            Err(ReadlineError::Interrupted) => {
                println!("CTRL-C");
                break
            },
            Err(ReadlineError::Eof) => {
                println!("CTRL-D");
                break
            },
            Err(err) => {
                println!("Error: {:?}", err);
                break
            }
        }
    }
    Ok(())
}