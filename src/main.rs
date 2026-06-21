mod encoder;
mod instr_table;
mod lexer;

use std::env;
use std::fs;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let program = args.first().map(|s| s.as_str()).unwrap_or("asmtool");

    let mut input_path: Option<String> = None;
    let mut output_path: Option<String> = None;
    let mut origin: u16 = 0xC100;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--output" => {
                i += 1;
                let Some(val) = args.get(i) else {
                    eprintln!("에러: {} 다음에 출력 경로가 와야함", args[i - 1]);
                    print_usage(program);
                    return ExitCode::FAILURE;
                };
                output_path = Some(val.clone());
            }
            "--origin" => {
                i += 1;
                let Some(val) = args.get(i) else {
                    eprintln!("에러: --origin 다음에 주소값이 와야함");
                    print_usage(program);
                    return ExitCode::FAILURE;
                };
                match parse_addr(val) {
                    Some(addr) => origin = addr,
                    None => {
                        eprintln!("에러: '{}'는 올바른 주소가 아님 (10진수 또는 0x16진수)", val);
                        return ExitCode::FAILURE;
                    }
                }
            }
            "-h" | "--help" => {
                print_usage(program);
                return ExitCode::SUCCESS;
            }
            other if input_path.is_none() => {
                input_path = Some(other.to_string());
            }
            other => {
                eprintln!("에러: 알 수 없는 인자 '{}'", other);
                print_usage(program);
                return ExitCode::FAILURE;
            }
        }
        i += 1;
    }

    let Some(input_path) = input_path else {
        eprintln!("에러: 입력 파일 안 줌");
        print_usage(program);
        return ExitCode::FAILURE;
    };

    let output_path = output_path.unwrap_or_else(|| default_output_path(&input_path));

    let source = match fs::read_to_string(&input_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("에러: '{}' 못 읽음 ({e})", input_path);
            return ExitCode::FAILURE;
        }
    };

    let bytes = match encoder::assemble(&source, origin) {
        Ok(b) => b,
        Err(e) => {
            // AsmError의 Display가 "N번째 줄: 메시지" 형태라 파일명만 앞에 붙임
            eprintln!("{input_path}: {e}");
            return ExitCode::FAILURE;
        }
    };

    if let Err(e) = fs::write(&output_path, &bytes) {
        eprintln!("에러: '{}'에 못 씀 ({e})", output_path);
        return ExitCode::FAILURE;
    }

    println!(
        "{input_path} → {output_path} ({} bytes, origin 0x{origin:04X})",
        bytes.len()
    );
    ExitCode::SUCCESS
}

fn print_usage(program: &str) {
    eprintln!("사용법: {program} <input.asm> [-o <output.bin>] [--origin <addr>]");
    eprintln!();
    eprintln!("  -o, --output <path>   출력 바이너리 경로 (기본값: <input>.bin)");
    eprintln!("  --origin <addr>       시작 주소, 10진수 또는 0x16진수 (기본값: 0xC100)");
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
