//! 2-pass 어셈블러.
//! pass 1: 모든 라인 훑으면서 라벨 → 주소 심볼테이블 채움 (encoded_len 기준으로 주소 누적)
//! pass 2: 심볼테이블 보면서 실제 바이트 생성. operand 해석은 전부 instr_table의 shape를 따라감.

use std::collections::HashMap;

use crate::instr_table::{find_by_mnemonic, Operand};
use crate::lexer::{lex, DataItem, LineKind};

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

        match &line.kind {
            Some(LineKind::Instr { mnemonic, .. }) => {
                let def = find_by_mnemonic(mnemonic).ok_or_else(|| AsmError {
                    line_no: line.line_no,
                    message: format!("알 수 없는 명령어 '{}'", mnemonic),
                })?;
                addr += def.encoded_len() as u32;
            }
            Some(LineKind::Db(items)) => {
                addr += data_byte_len(items, 1) as u32;
            }
            Some(LineKind::Dw(items)) => {
                addr += data_byte_len(items, 2) as u32;
            }
            None => {}
        }

        if addr > 0x10000 {
            return Err(AsmError {
                line_no: line.line_no,
                message: "64KB 메모리 범위 초과".to_string(),
            });
        }
    }

    // ── pass 2: 실제 인코딩 ──
    let mut out = Vec::new();

    for line in &lines {
        match &line.kind {
            None => {}
            Some(LineKind::Instr { mnemonic, operands }) => {
                let def = find_by_mnemonic(mnemonic).unwrap();

                if operands.len() != def.operands.len() {
                    return Err(AsmError {
                        line_no: line.line_no,
                        message: format!(
                            "'{}'는 operand {}개 필요한데 {}개 받음",
                            mnemonic,
                            def.operands.len(),
                            operands.len()
                        ),
                    });
                }

                out.push(def.opcode);
                for (token, shape) in operands.iter().zip(def.operands.iter()) {
                    encode_operand(token, *shape, &symbols, line.line_no, &mut out)?;
                }
            }
            Some(LineKind::Db(items)) => {
                encode_data_items(items, 1, &symbols, line.line_no, &mut out)?;
            }
            Some(LineKind::Dw(items)) => {
                encode_data_items(items, 2, &symbols, line.line_no, &mut out)?;
            }
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

/// pass1용: items가 차지할 바이트 수 계산 (unit = 1이면 db, 2면 dw)
fn data_byte_len(items: &[DataItem], unit: usize) -> usize {
    items.iter().map(|item| match item {
        DataItem::Value(_) => unit,
        DataItem::Str(s) => s.len(), // db 전용 — dw에서 문자열 쓰면 encode 단계에서 에러냄
    }).sum()
}

/// pass2용: items를 실제 바이트로 인코딩
fn encode_data_items(
    items: &[DataItem],
    unit: usize,
    symbols: &HashMap<String, u16>,
    line_no: usize,
    out: &mut Vec<u8>,
) -> Result<(), AsmError> {
    for item in items {
        match item {
            DataItem::Str(s) => {
                if unit == 2 {
                    return Err(AsmError {
                        line_no,
                        message: "dw에는 문자열 리터럴 못 씀, db 써야 함".to_string(),
                    });
                }
                // 이스케이프 시퀀스 처리
                let mut chars = s.chars().peekable();
                while let Some(c) = chars.next() {
                    let byte = if c == '\\' {
                        match chars.next() {
                            Some('n')  => b'\n',
                            Some('t')  => b'\t',
                            Some('0')  => b'\0',
                            Some('\\') => b'\\',
                            Some('"')  => b'"',
                            Some(other) => other as u8,
                            None => b'\\',
                        }
                    } else {
                        c as u8
                    };
                    out.push(byte);
                }
            }
            DataItem::Value(token) => {
                let value = resolve_value(token, symbols, line_no)?;
                if unit == 1 {
                    if value > 0xFF {
                        return Err(AsmError {
                            line_no,
                            message: format!("db: '{token}'는 8비트 범위 초과"),
                        });
                    }
                    out.push(value as u8);
                } else {
                    if value > 0xFFFF {
                        return Err(AsmError {
                            line_no,
                            message: format!("dw: '{token}'는 16비트 범위 초과"),
                        });
                    }
                    out.push((value & 0xFF) as u8);
                    out.push((value >> 8) as u8);
                }
            }
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
