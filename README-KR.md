[![Crates.io](https://img.shields.io/crates/v/rustVM-Assembler)](https://crates.io/crates/rustVM-Aseembler)
[![License](https://img.shields.io/github/license/Jjoon0513/rustVM-Assembler)](LICENSE)


[English](README.md) | *한국어*

# Rust Vm Assembler

`rustVm-Assembler`는 16비트 **rustVM** 명령어 집합을 위한 2패스 어셈블러입니다.

NASM과 유사한 문법을 사용하며, 인코더와 명령어 테이블은 rustVM에 맞게 직접 설계되었습니다.

---

## 주요 기능

- 2패스 어셈블 (심볼 분석 + 바이너리 생성)
- NASM 스타일 문법
- `db` / `dw` 지시어 지원
- 확장하기 쉬운 Lexer, Encoder, Opcode Table 구조
- 명령줄(CLI) 인터페이스
- VS Code 문법 강조 확장 ([rva-syntax](https://github.com/Jjoon0513/rva-syntax))

---

## 설치

### crates.io에서 설치

```bash
cargo install rustvm-assembler
```

### 소스 코드에서 설치

```bash
git clone https://github.com/Jjoon0513/rustVM-Assembler.git
cd rustVM-Assembler
cargo install --path .
```

설치가 완료되었는지 확인합니다.

```bash
rva --version
```

---

## 사용법

어셈블리 파일을 바이너리로 변환합니다.

```bash
rva input.asm
```

실행하면 다음 파일이 생성됩니다.

```
input.bin
```

출력 파일 이름을 지정하려면 다음과 같이 실행합니다.

```bash
rva input.asm -o output.bin
```

사용 가능한 모든 옵션을 확인하려면 다음 명령을 실행합니다.

```bash
rva --help
```

---

## 예제

**hello.asm**

```asm
start:
    movi r0, 123
    hlt
```

다음 명령으로 어셈블합니다.

```bash
rva hello.asm -o hello.bin
```

생성된 `hello.bin` 파일은
        [rustVM](https://github.com/Jjoon0513/rustVM)에서 실행할 수 있습니다.

---

## 지원하는 지시어

| 지시어 | 설명 |
| ------- | ---- |
| `db` | 하나 이상의 바이트를 삽입합니다. |
| `dw` | 하나 이상의 16비트 워드를 삽입합니다. |

예시:

```asm
db 0x41, 0x42, 0x43
dw 0x1234
```

---

## 프로젝트 구조

```
rva/
├── src/
│   ├── lexer.rs
│   ├── encoder.rs
│   ├── opcode_table.rs
│   └── main.rs
├── tests/
└── README.md
```

---

## 관련 프로젝트

- [rustVM](https://github.com/Jjoon0513/rustVM)

---

## 개발 예정

- [ ] 줄/열 정보를 포함한 오류 메시지 개선
- [ ] 여러 파일을 어셈블하는 기능
- [ ] 심볼 공개(Visibility) 지원
- [ ] 목적(Object) 파일 생성 지원

---

## 라이선스

MIT OR Apache-2.0
