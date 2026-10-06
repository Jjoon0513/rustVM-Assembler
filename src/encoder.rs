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
        write!(f, "Line {}: {}", self.line_no, self.message)
    }
}

/// origin: 첫 명령어가 배치될 메모리 주소 (예: 커널 코드면 0xC100)
pub fn assemble(source: &str, origin: u16) -> Result<Vec<u8>, AsmError> {
    let lines = lex(source).map_err(|e| AsmError {
        line_no: e.line_no,
        message: e.message,
    })?;

    // ── pass 1: 라벨 주소 계산 ──
    let mut symbols: HashMap<String, u16> = HashMap::new();
    let mut addr: u32 = origin as u32;

    for line in &lines {
        if let Some(label) = &line.label {
            if symbols.contains_key(label) {
                return Err(AsmError {
                    line_no: line.line_no,
                        message: format!("Label '{}' already defined", label),
                });
            }
            symbols.insert(label.clone(), addr as u16);
        }

        match &line.kind {
            Some(LineKind::Instr { mnemonic, .. }) => {
                let def = find_by_mnemonic(mnemonic).ok_or_else(|| AsmError {
                    line_no: line.line_no,
                    message: format!("Unknown instruction '{}'", mnemonic),
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
                    message: "Exceeds 64KB memory range".to_string(),
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
                            "'{}' requires {} operands but got {}",
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
                message: format!("'{}' is not a valid register (r0~r17, zr, sp)", token),
            })?;
            out.push(reg);
        }
        Operand::Imm8 => {
            let value = resolve_value(token, symbols, line_no)?;
            if !(-128..=0xFF).contains(&value) {
                return Err(AsmError {
                    line_no,
                    message: format!("'{}' exceeds 8-bit range (-128..255)", token),
                });
            }
            out.push(value as u8); // 음수는 2의 보수로 잘림
        }
        Operand::Imm16 => {
            let value = resolve_value(token, symbols, line_no)?;
            if !(-0x8000..=0xFFFF).contains(&value) {
                return Err(AsmError {
                    line_no,
                    message: format!("'{}' exceeds 16-bit range (-32768..65535)", token),
                });
            }
            let value = value as u16; // 음수는 2의 보수
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
        DataItem::Str(bytes) => bytes.len(), // db 전용 — dw에서 문자열 쓰면 encode 단계에서 에러냄
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
            DataItem::Str(bytes) => {
                if unit == 2 {
                    return Err(AsmError {
                        line_no,
                        message: "String literals are not allowed in 'dw'; use 'db' instead".to_string(),
                    });
                }
                // 이스케이프/UTF-8 인코딩은 렉서에서 끝났으므로 그대로 출력 (pass 1 길이와 항상 일치)
                out.extend_from_slice(bytes);
            }
            DataItem::Value(token) => {
                let value = resolve_value(token, symbols, line_no)?;
                if unit == 1 {
                    if !(-128..=0xFF).contains(&value) {
                        return Err(AsmError {
                            line_no,
                            message: format!("db: '{token}' exceeds 8-bit range"),
                        });
                    }
                    out.push(value as u8);
                } else {
                    if !(-0x8000..=0xFFFF).contains(&value) {
                        return Err(AsmError {
                            line_no,
                            message: format!("dw: '{token}' exceeds 16-bit range"),
                        });
                    }
                    let value = value as u16;
                    out.push((value & 0xFF) as u8);
                    out.push((value >> 8) as u8);
                }
            }
        }
    }
    Ok(())
}

/// r0~r17, 그리고 별칭 zr(=r16, 제로 레지스터) / sp(=r17, 스택 포인터)
fn parse_register(token: &str) -> Option<u8> {
    let lower = token.to_lowercase();
    match lower.as_str() {
        "zr" => return Some(16),
        "sp" => return Some(17),
        _ => {}
    }
    let digits = lower.strip_prefix('r')?;
    let n: u8 = digits.parse().ok()?;
    (n <= 17).then_some(n)
}

/// 10진수 / 0x16진수 숫자 리터럴(앞에 `-` 허용), 또는 라벨 이름을 값으로 변환
fn resolve_value(token: &str, symbols: &HashMap<String, u16>, line_no: usize) -> Result<i64, AsmError> {
    let (neg, body) = match token.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, token),
    };

    let parsed = if let Some(hex) = body.strip_prefix("0x").or_else(|| body.strip_prefix("0X")) {
        Some(i64::from_str_radix(hex, 16).map_err(|_| AsmError {
            line_no,
            message: format!("'{}' is not a valid hexadecimal number", token),
        })?)
    } else {
        body.parse::<i64>().ok()
    };

    if let Some(n) = parsed {
        return Ok(if neg { -n } else { n });
    }

    if !neg {
        if let Some(&a) = symbols.get(token) {
            return Ok(a as i64);
        }
    }

    Err(AsmError {
        line_no,
        message: format!("'{}' is neither a number nor a defined label", token),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asm(src: &str) -> Vec<u8> {
        assemble(src, 0).expect("assemble failed")
    }
    fn asm_err(src: &str) -> AsmError {
        assemble(src, 0).expect_err("expected an error")
    }

    // ── 레지스터 ──
    #[test]
    fn zr_sp_aliases_and_numbers() {
        assert_eq!(asm("movr r0, zr"), [0x09, 0, 16]);
        assert_eq!(asm("movr sp, r1"), [0x09, 17, 1]);
        assert_eq!(asm("movr R16, R17"), [0x09, 16, 17]);
        assert_eq!(asm("push SP\npop ZR"), [0x40, 17, 0x41, 16]);
    }
    #[test]
    fn register_out_of_range_is_error() {
        assert!(asm_err("movr r0, r18").message.contains("not a valid register"));
        assert!(asm_err("movr r0, r256").message.contains("not a valid register"));
    }

    // ── 음수 즉시값 ──
    #[test]
    fn negative_immediates() {
        assert_eq!(asm("addi r0, -1"), [0x18, 0, 0xFF, 0xFF]);
        assert_eq!(asm("subi r1, -0x10"), [0x1A, 1, 0xF0, 0xFF]);
        assert_eq!(asm("movi r0, -32768"), [0x08, 0, 0x00, 0x80]);
        assert_eq!(asm("shli r0, -1"), [0x60, 0, 0xFF]);
        assert_eq!(asm("db -1"), [0xFF]);
        assert_eq!(asm("dw -2"), [0xFE, 0xFF]);
    }
    #[test]
    fn immediate_range_errors() {
        assert!(asm_err("movi r0, -32769").message.contains("exceeds 16-bit"));
        assert!(asm_err("movi r0, 65536").message.contains("exceeds 16-bit"));
        assert!(asm_err("shli r0, 256").message.contains("exceeds 8-bit"));
        assert!(asm_err("db -129").message.contains("exceeds 8-bit"));
        assert!(asm_err("movi r0, -foo").message.contains("neither a number"));
    }
    #[test]
    fn existing_encoding_unchanged() {
        assert_eq!(asm("movi r0, 0x1234"), [0x08, 0, 0x34, 0x12]);
        assert_eq!(asm("x: jmp x"), [0x30, 0, 0]);
    }

    // ── 문자열 ──
    #[test]
    fn semicolon_and_slashes_inside_string_are_not_comments() {
        assert_eq!(asm(r#"db "ab;c", 7"#), [b'a', b'b', b';', b'c', 7]);
        assert_eq!(asm(r#"db "http://x""#), b"http://x");
    }
    #[test]
    fn comment_after_string_is_still_stripped() {
        assert_eq!(asm(r#"db "a" ; comment"#), [b'a']);
        assert_eq!(asm(r#"db "a;b" // comment"#), b"a;b");
    }
    #[test]
    fn escapes() {
        assert_eq!(
            asm(r#"db "a\nb\t\0\\\"\x41\r""#),
            [b'a', 0x0A, b'b', 0x09, 0x00, b'\\', b'"', 0x41, 0x0D]
        );
    }
    #[test]
    fn escaped_quote_does_not_end_string_or_comment_scan() {
        assert_eq!(asm(r#"db "a\";b" ; real comment"#), b"a\";b");
    }
    #[test]
    fn utf8_string_length_and_label_agree() {
        // "한" = ED 95 9C (3바이트). end 라벨은 3이어야 한다.
        assert_eq!(asm("db \"한\"\nend: dw end"), [0xED, 0x95, 0x9C, 0x03, 0x00]);
    }
    #[test]
    fn escape_length_and_label_agree() {
        // "\n" 은 1바이트. end 라벨은 3이어야 한다.
        assert_eq!(asm("db \"a\\nb\"\nend: dw end"), [b'a', 0x0A, b'b', 0x03, 0x00]);
    }
    #[test]
    fn string_errors() {
        let e = asm_err("nop\ndb \"abc");
        assert_eq!(e.line_no, 2);
        assert!(e.message.contains("unterminated"));
        assert!(asm_err(r#"db "a\qb""#).message.contains("unknown escape"));
        assert!(asm_err(r#"db "\xZZ""#).message.contains("\\x escape"));
        assert!(asm_err(r#"dw "ab""#).message.contains("not allowed in 'dw'"));
    }
    #[test]
    fn label_with_colon_in_string() {
        assert_eq!(asm("msg: db \"a:b\"\njmp msg"), [b'a', b':', b'b', 0x30, 0, 0]);
    }
}
