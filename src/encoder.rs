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

    // ── ISA v2 ──
    // 아래 바이트열은 rustVM 쪽 `isa_v2_tests` 의 인코딩과 **동일**해야 한다 (양쪽 동기화 체크).
    #[test]
    fn isa_v2_carry_and_shift() {
        assert_eq!(asm("adci r1, 0"), [0x10, 1, 0, 0]);
        assert_eq!(asm("adcr r1, r3"), [0x11, 1, 3]);
        assert_eq!(asm("sbbi r1, 0"), [0x12, 1, 0, 0]);
        assert_eq!(asm("sbbr r1, r3"), [0x13, 1, 3]);
        assert_eq!(asm("sari r0, 1"), [0x64, 0, 1]);
        assert_eq!(asm("sarr r4, r5"), [0x65, 4, 5]);
    }
    #[test]
    fn isa_v2_indirect_jump_and_call() {
        assert_eq!(asm("jmpr r0"), [0x3B, 0]);
        assert_eq!(asm("callr r7"), [0x4A, 7]);
        assert_eq!(asm("jmpr sp"), [0x3B, 17]);
    }
    #[test]
    fn isa_v2_byte_and_offset_memory() {
        assert_eq!(asm("loadb r1, r0"), [0x54, 1, 0]);
        assert_eq!(asm("storeb r3, r2"), [0x58, 3, 2]);
        assert_eq!(asm("loado r1, r0, 2"), [0x55, 1, 0, 2, 0]);
        assert_eq!(asm("loado r2, r0, -4"), [0x55, 2, 0, 0xFC, 0xFF]);
        assert_eq!(asm("loadbo r3, r0, -3"), [0x56, 3, 0, 0xFD, 0xFF]);
        assert_eq!(asm("storeo r0, -4, r4"), [0x59, 0, 0xFC, 0xFF, 4]);
        assert_eq!(asm("storebo r0, -3, r5"), [0x5A, 0, 0xFD, 0xFF, 5]);
        // 스택 프레임 접근 (SP 기준)
        assert_eq!(asm("loado r0, sp, 4"), [0x55, 0, 17, 4, 0]);
        assert_eq!(asm("storeo sp, -2, r1"), [0x59, 17, 0xFE, 0xFF, 1]);
    }
    #[test]
    fn isa_v2_zr_destination_and_operand_count() {
        assert_eq!(asm("loado zr, r0, 0"), [0x55, 0x10, 0, 0, 0]);
        assert!(asm_err("loado r1, r0").message.contains("requires 3 operands"));
        // 피연산자 순서/형태가 (Reg, Imm16, Reg)에 맞지 않으면 에러
        assert!(asm_err("storeo r0, r1, r2").message.contains("neither a number"));
        assert!(asm_err("storeo r0, 4, 5").message.contains("not a valid register"));
    }
    #[test]
    fn isa_v2_label_as_offset_base_and_branch_target() {
        // 라벨은 Imm16 자리에 쓸 수 있다 (jmpr 은 레지스터라 movi로 주소를 만든다)
        assert_eq!(asm("movi r0, f\njmpr r0\nf: nop"), [0x08, 0, 6, 0, 0x3B, 0, 0x00]);
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
