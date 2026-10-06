//! 단일 source of truth: step.rs 디스패치 + 어셈블러 인코더가 이 테이블 하나만 보게 만드는 게 목표.
//! step.rs랑 exec/*.rs 핸들러들 까서 실제 operand를 몇 바이트, 어떤 순서로 읽는지 직접 추적해서 만듦.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operand {
    /// 레지스터 번호 1바이트 (0~15), fetch_u8 1번
    Reg,
    /// 1바이트 즉시값 (shift amount, interrupt 번호). fetch_u8 1번, Reg랑 바이트 레벨에선 동일하지만 의미가 다름
    Imm8,
    /// 2바이트 즉시값. get_high_low()과 동일하게 LOW byte 먼저, HIGH byte 다음
    Imm16,
}

impl Operand {
    pub const fn size(&self) -> usize {
        match self {
            Operand::Reg | Operand::Imm8 => 1,
            Operand::Imm16 => 2,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct InstrDef {
    pub mnemonic: &'static str,
    pub opcode: u8,
    pub operands: &'static [Operand],
}

impl InstrDef {
    /// opcode 1바이트 + operand 바이트 전부 합친 명령어 총 길이
    pub const fn encoded_len(&self) -> usize {
        let mut len = 1;
        let mut i = 0;
        while i < self.operands.len() {
            len += self.operands[i].size();
            i += 1;
        }
        len
    }
}

use Operand::*;

pub const INSTRUCTIONS: &[InstrDef] = &[
    // ── 0x00 ~ 0x07 시스템 ──
    InstrDef { mnemonic: "nop",     opcode: 0x00, operands: &[] },
    InstrDef { mnemonic: "syscall", opcode: 0x01, operands: &[] },
    InstrDef { mnemonic: "sysret",  opcode: 0x02, operands: &[] },
    InstrDef { mnemonic: "int",     opcode: 0x03, operands: &[Imm8] },
    InstrDef { mnemonic: "hlt",     opcode: 0x04, operands: &[] },
    InstrDef { mnemonic: "cli",     opcode: 0x05, operands: &[] },
    InstrDef { mnemonic: "sti",     opcode: 0x06, operands: &[] },
    InstrDef { mnemonic: "iret",    opcode: 0x07, operands: &[] },

    // ── 0x08 ~ 0x09 MOV ──
    InstrDef { mnemonic: "movi",    opcode: 0x08, operands: &[Reg, Imm16] },
    InstrDef { mnemonic: "movr",    opcode: 0x09, operands: &[Reg, Reg] },

    // ── ISA v2: 0x10 ~ 0x13 ADC/SBB (캐리/빌림 포함 연산, 다중 워드 산술용) ──
    InstrDef { mnemonic: "adci",    opcode: 0x10, operands: &[Reg, Imm16] },
    InstrDef { mnemonic: "adcr",    opcode: 0x11, operands: &[Reg, Reg] },
    InstrDef { mnemonic: "sbbi",    opcode: 0x12, operands: &[Reg, Imm16] },
    InstrDef { mnemonic: "sbbr",    opcode: 0x13, operands: &[Reg, Reg] },

    // ── 0x18 ~ 0x1C ADD/SUB/CMP ──
    InstrDef { mnemonic: "addi",    opcode: 0x18, operands: &[Reg, Imm16] },
    InstrDef { mnemonic: "addr",    opcode: 0x19, operands: &[Reg, Reg] },
    InstrDef { mnemonic: "subi",    opcode: 0x1A, operands: &[Reg, Imm16] },
    InstrDef { mnemonic: "subr",    opcode: 0x1B, operands: &[Reg, Reg] },
    InstrDef { mnemonic: "cmp",     opcode: 0x1C, operands: &[Reg, Reg] },

    // ── 0x20 ~ 0x23 MUL/DIV ── lhs/dividend, 결과 전부 R12:R13 고정 레지스터라 operand 없음
    InstrDef { mnemonic: "muli",    opcode: 0x20, operands: &[Imm16] },
    InstrDef { mnemonic: "mulr",    opcode: 0x21, operands: &[Reg] },
    InstrDef { mnemonic: "divi",    opcode: 0x22, operands: &[Imm16] },
    InstrDef { mnemonic: "divr",    opcode: 0x23, operands: &[Reg] },

    // ── 0x28 ~ 0x2E AND/OR/XOR/NOT ──
    InstrDef { mnemonic: "and",     opcode: 0x28, operands: &[Reg, Reg] },
    InstrDef { mnemonic: "or",      opcode: 0x29, operands: &[Reg, Reg] },
    InstrDef { mnemonic: "xor",     opcode: 0x2A, operands: &[Reg, Reg] },
    InstrDef { mnemonic: "andi",    opcode: 0x2B, operands: &[Reg, Imm16] },
    InstrDef { mnemonic: "ori",     opcode: 0x2C, operands: &[Reg, Imm16] },
    InstrDef { mnemonic: "xori",    opcode: 0x2D, operands: &[Reg, Imm16] },
    InstrDef { mnemonic: "not",     opcode: 0x2E, operands: &[Reg] },

    // ── 0x30 ~ 0x3A 분기 (전부 imm16 절대주소) ──
    InstrDef { mnemonic: "jmp",     opcode: 0x30, operands: &[Imm16] },
    InstrDef { mnemonic: "je",      opcode: 0x31, operands: &[Imm16] },
    InstrDef { mnemonic: "jne",     opcode: 0x32, operands: &[Imm16] },
    InstrDef { mnemonic: "ja",      opcode: 0x33, operands: &[Imm16] },
    InstrDef { mnemonic: "jae",     opcode: 0x34, operands: &[Imm16] },
    InstrDef { mnemonic: "jb",      opcode: 0x35, operands: &[Imm16] },
    InstrDef { mnemonic: "jbe",     opcode: 0x36, operands: &[Imm16] },
    InstrDef { mnemonic: "jg",      opcode: 0x37, operands: &[Imm16] },
    InstrDef { mnemonic: "jge",     opcode: 0x38, operands: &[Imm16] },
    InstrDef { mnemonic: "jl",      opcode: 0x39, operands: &[Imm16] },
    InstrDef { mnemonic: "jle",     opcode: 0x3A, operands: &[Imm16] },

    // ── ISA v2: 0x3B 간접 점프 ──
    InstrDef { mnemonic: "jmpr",     opcode: 0x3B, operands: &[Reg] },

    // ── 0x40 ~ 0x41 스택 ──
    InstrDef { mnemonic: "push",    opcode: 0x40, operands: &[Reg] },
    InstrDef { mnemonic: "pop",     opcode: 0x41, operands: &[Reg] },

    // ── 0x48 ~ 0x49 CALL/RET ──
    InstrDef { mnemonic: "call",    opcode: 0x48, operands: &[Imm16] },
    InstrDef { mnemonic: "ret",     opcode: 0x49, operands: &[] },
    // ISA v2: 간접 호출 (프레임은 call과 동일)
    InstrDef { mnemonic: "callr",   opcode: 0x4A, operands: &[Reg] },

    // ── 0x50 ~ 0x53 LOAD/STORE ──
    // load 계열: dst가 항상 1번 operand. store 계열: addr가 항상 1번 operand. (아래 한계점 참고)
    InstrDef { mnemonic: "loadr",   opcode: 0x50, operands: &[Reg, Reg] },
    InstrDef { mnemonic: "loadi",   opcode: 0x51, operands: &[Reg, Imm16] },
    InstrDef { mnemonic: "storer",  opcode: 0x52, operands: &[Reg, Reg] },
    InstrDef { mnemonic: "storei",  opcode: 0x53, operands: &[Imm16, Reg] },

    // ── ISA v2: 바이트 / 오프셋 load·store ──
    // 오프셋(Imm16)은 부호 있는 16비트로 해석되어 base + off 가 16비트 래핑 덧셈으로 계산됨 (예: -4 → base-4).
    // 바이트 load는 zero-extend, 바이트 store는 src의 하위 바이트만 씀.
    InstrDef { mnemonic: "loadb",   opcode: 0x54, operands: &[Reg, Reg] },        // dst, addr
    InstrDef { mnemonic: "loado",   opcode: 0x55, operands: &[Reg, Reg, Imm16] }, // dst, base, off
    InstrDef { mnemonic: "loadbo",  opcode: 0x56, operands: &[Reg, Reg, Imm16] }, // dst, base, off
    InstrDef { mnemonic: "storeb",  opcode: 0x58, operands: &[Reg, Reg] },        // addr, src
    InstrDef { mnemonic: "storeo",  opcode: 0x59, operands: &[Reg, Imm16, Reg] }, // base, off, src
    InstrDef { mnemonic: "storebo", opcode: 0x5A, operands: &[Reg, Imm16, Reg] }, // base, off, src

    // ── 0x60 ~ 0x63 SHIFT ── amount는 Imm16이 아니라 Imm8임
    InstrDef { mnemonic: "shli",    opcode: 0x60, operands: &[Reg, Imm8] },
    InstrDef { mnemonic: "shlr",    opcode: 0x61, operands: &[Reg, Reg] },
    InstrDef { mnemonic: "shri",    opcode: 0x62, operands: &[Reg, Imm8] },
    InstrDef { mnemonic: "shrr",    opcode: 0x63, operands: &[Reg, Reg] },
    // ISA v2: 산술 우시프트 (부호 유지, amount ≥ 16 이면 부호 비트로 가득 참)
    InstrDef { mnemonic: "sari",    opcode: 0x64, operands: &[Reg, Imm8] },
    InstrDef { mnemonic: "sarr",    opcode: 0x65, operands: &[Reg, Reg] },
];

/// 어셈블러가 mnemonic 파싱 후 호출
pub fn find_by_mnemonic(mnemonic: &str) -> Option<&'static InstrDef> {
    INSTRUCTIONS.iter().find(|i| i.mnemonic == mnemonic)
}

/// step.rs 디스패치나 디스어셈블러가 호출
#[allow(dead_code)]
pub fn find_by_opcode(opcode: u8) -> Option<&'static InstrDef> {
    INSTRUCTIONS.iter().find(|i| i.opcode == opcode)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn opcodes_and_mnemonics_are_unique() {
        let mut ops = HashSet::new();
        let mut names = HashSet::new();
        for i in INSTRUCTIONS {
            assert!(ops.insert(i.opcode), "opcode 중복: {:#04x} ({})", i.opcode, i.mnemonic);
            assert!(names.insert(i.mnemonic), "mnemonic 중복: {}", i.mnemonic);
        }
    }

    #[test]
    fn isa_v2_encoded_lengths() {
        let len = |m: &str| find_by_mnemonic(m).unwrap().encoded_len();
        assert_eq!(len("adci"), 4);
        assert_eq!(len("adcr"), 3);
        assert_eq!(len("jmpr"), 2);
        assert_eq!(len("callr"), 2);
        assert_eq!(len("loadb"), 3);
        assert_eq!(len("loado"), 5);
        assert_eq!(len("storeo"), 5);
        assert_eq!(len("sari"), 3);
    }
}
