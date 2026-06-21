//! 2-pass 어셈블러.
//! pass 1: 모든 라인 훑으면서 라벨 → 주소 심볼테이블 채움 (encoded_len 기준으로 주소 누적)
//! pass 2: 심볼테이블 보면서 실제 바이트 생성. operand 해석은 전부 instr_table의 shape를 따라감.

use std::collections::HashMap;

use crate::instr_table::{find_by_mnemonic, Operand};
use crate::lexer::lex;

#[derive(Debug)]
pub struct AsmError {
    pub line_no: usize,
    pub message: String,
}

impl std::fmt::Display for AsmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}번째 줄: {}", self.line_no, self.message)
    }
}

/// origin: 첫 명령어가 배치될 메모리 주소 (예: 커널 코드면 0xC100)
pub fn assemble(source: &str, origin: u16) -> Result<Vec<u8>, AsmError> {
    let lines = lex(source);

    // ── pass 1: 라벨 주소 계산 ──
    let mut symbols: HashMap<String, u16> = HashMap::new();
    let mut addr: u32 = origin as u32;

    for line in &lines {
        if let Some(label) = &line.label {
            if symbols.contains_key(label) {
                return Err(AsmError {
                    line_no: line.line_no,
                    message: format!("라벨 '{}' 중복 정의", label),
                });
            }
            symbols.insert(label.clone(), addr as u16);
        }

        if let Some(mnemonic) = &line.mnemonic {
            let def = find_by_mnemonic(mnemonic).ok_or_else(|| AsmError {
                line_no: line.line_no,
                message: format!("알 수 없는 명령어 '{}'", mnemonic),
            })?;
            addr += def.encoded_len() as u32;

            if addr > 0x10000 {
                return Err(AsmError {
                    line_no: line.line_no,
                    message: "64KB 메모리 범위 초과".to_string(),
                });
            }
        }
    }

    // ── pass 2: 실제 인코딩 ──
    let mut out = Vec::new();

    for line in &lines {
        let Some(mnemonic) = &line.mnemonic else { continue };
        // pass1에서 이미 find_by_mnemonic 검증 통과한 라인만 여기 옴
        let def = find_by_mnemonic(mnemonic).unwrap();

        if line.operands.len() != def.operands.len() {
            return Err(AsmError {
                line_no: line.line_no,
                message: format!(
                    "'{}'는 operand {}개 필요한데 {}개 받음",
                    mnemonic,
                    def.operands.len(),
                    line.operands.len()
                ),
            });
        }

        out.push(def.opcode);

        for (token, shape) in line.operands.iter().zip(def.operands.iter()) {
            encode_operand(token, *shape, &symbols, line.line_no, &mut out)?;
        }
    }

    Ok(out)
}

fn encode_operand(
    token: &str,
    shape: Operand,
    symbols: &HashMap<String, u16>,
    line_no: usize,
    out: &mut Vec<u8>,
) -> Result<(), AsmError> {
    match shape {
        Operand::Reg => {
            let reg = parse_register(token).ok_or_else(|| AsmError {
                line_no,
                message: format!("'{}'는 올바른 레지스터가 아님 (r0~r15)", token),
            })?;
            out.push(reg);
        }
        Operand::Imm8 => {
            let value = resolve_value(token, symbols, line_no)?;
            if value > 0xFF {
                return Err(AsmError {
                    line_no,
                    message: format!("'{}'는 8비트 범위(0~255) 초과", token),
                });
            }
            out.push(value as u8);
        }
        Operand::Imm16 => {
            let value = resolve_value(token, symbols, line_no)?;
            if value > 0xFFFF {
                return Err(AsmError {
                    line_no,
                    message: format!("'{}'는 16비트 범위(0~65535) 초과", token),
                });
            }
            // vm.rs의 get_high_low()가 LOW 먼저 읽고 HIGH 나중에 읽으니까 그 순서 그대로
            out.push((value & 0xFF) as u8);
            out.push((value >> 8) as u8);
        }
    }
    Ok(())
}

fn parse_register(token: &str) -> Option<u8> {
    let digits = token.to_lowercase();
    let digits = digits.strip_prefix('r')?;
    let n: u8 = digits.parse().ok()?;
    (n <= 15).then_some(n)
}

/// 10진수 / 0x16진수 숫자 리터럴, 또는 라벨 이름을 주소값으로 변환
fn resolve_value(token: &str, symbols: &HashMap<String, u16>, line_no: usize) -> Result<u32, AsmError> {
    if let Some(hex) = token.strip_prefix("0x").or_else(|| token.strip_prefix("0X")) {
        return u32::from_str_radix(hex, 16).map_err(|_| AsmError {
            line_no,
            message: format!("'{}'는 올바른 16진수가 아님", token),
        });
    }

    if let Ok(n) = token.parse::<u32>() {
        return Ok(n);
    }

    symbols.get(token).map(|&a| a as u32).ok_or_else(|| AsmError {
        line_no,
        message: format!("'{}'는 숫자도 아니고 정의된 라벨도 아님", token),
    })
}
