mod encoder;
mod instr_table;
mod lexer;

use std::env;
use std::fs;
use std::io::Write; // ADDED
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let program = args.first().map(|s| s.as_str()).unwrap_or("asmtool");

    let program_name = std::path::Path::new(program)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(program);

    let mut input_path: Option<String> = None;
    let mut output_path: Option<String> = None;
    let mut origin: u16 = 0xC100;
    let mut stdout_mode = false; // ADDED
    let version = "0.1.1";

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--output" => {
                i += 1;
                let Some(val) = args.get(i) else {
                    eprintln!("Error: {} requires an output path", args[i - 1]);
                    print_usage(program_name);
                    return ExitCode::FAILURE;
                };
                output_path = Some(val.clone());
            }

            "--stdout" => {
                stdout_mode = true;
            }

            "--origin" => {
                i += 1;
                let Some(val) = args.get(i) else {
                    eprintln!("Error: --origin requires an address");
                    print_usage(program_name);
                    return ExitCode::FAILURE;
                };

                match parse_addr(val) {
                    Some(addr) => origin = addr,
                    None => {
                        eprintln!(
                            "Error: '{}' is not a valid address (decimal or 0xhex)",
                            val
                        );
                        return ExitCode::FAILURE;
                    }
                }
            }

            "-v" | "--version" => {
                println!("RustVm-Assembler: {version}");
                return ExitCode::SUCCESS;
            }

            "-h" | "--help" => {
                print_usage(program_name);
                return ExitCode::SUCCESS;
            }

            other if input_path.is_none() => {
                input_path = Some(other.to_string());
            }

            other => {
                eprintln!("Error: unknown argument '{}'", other);
                print_usage(program_name);
                return ExitCode::FAILURE;
            }
        }

        i += 1;
    }

    // ADDED
    if stdout_mode && output_path.is_some() {
        eprintln!("Error: cannot use --stdout and -o/--output together");
        return ExitCode::FAILURE;
    }

    let Some(input_path) = input_path else {
        eprintln!("Error: no input file provided");
        print_usage(program_name);
        return ExitCode::FAILURE;
    };

    let output_path = output_path.unwrap_or_else(|| default_output_path(&input_path));

    let source = match fs::read_to_string(&input_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: failed to read '{}': {e}", input_path);
            return ExitCode::FAILURE;
        }
    };

    let bytes = match encoder::assemble(&source, origin) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{input_path}: {e}");
            return ExitCode::FAILURE;
        }
    };

    // CHANGED
    if stdout_mode {
        if let Err(e) = std::io::stdout().write_all(&bytes) {
            eprintln!("Error: failed to write to stdout: {e}");
            return ExitCode::FAILURE;
        }
    } else {
        if let Err(e) = fs::write(&output_path, &bytes) {
            eprintln!("Error: failed to write to '{}': {e}", output_path);
            return ExitCode::FAILURE;
        }

        println!(
            "{input_path} → {output_path} ({} bytes, origin 0x{origin:04X})",
            bytes.len()
        );
    }

    ExitCode::SUCCESS
}

fn print_usage(program: &str) {
    eprintln!("Usage: {program} <input.asm> [-o <output.bin>] [--origin <addr>] [--stdout]");
    eprintln!();
    eprintln!("  -o, --output <path>   Output binary path (default: <input>.bin)");
    eprintln!("  --origin <addr>       Start address, decimal or 0xhex (default: 0xC100)");
    eprintln!("  --stdout              Write raw binary to stdout");
}

fn parse_addr(s: &str) -> Option<u16> {
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u16::from_str_radix(hex, 16).ok()
    } else {
        s.parse::<u16>().ok()
    }
}

fn default_output_path(input: &str) -> String {
    match input.rfind('.') {
        Some(idx) => format!("{}.bin", &input[..idx]),
        None => format!("{input}.bin"),
    }
}