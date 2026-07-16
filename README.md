[![Crates.io](https://img.shields.io/crates/v/rustVM-Assembler)](https://crates.io/crates/rustVM-Aseembler)
[![License](https://img.shields.io/github/license/Jjoon0513/rustVM-Assembler)](LICENSE)


*English* | [한국어](README-ko.md)
# Rust Vm Assembler

`RustVm-Assembler` is a two-pass assembler for the 16-bit **rustVM** instruction set.

It uses a NASM-inspired syntax while providing an encoder and opcode table specifically designed for rustVM.

---

## Features

- Two-pass assembly (symbol resolution + binary generation)
- NASM-inspired syntax
- `db` / `dw` directives
- Easy-to-extend lexer, encoder, and opcode table
- Command-line interface
- VS Code syntax highlighting extension ([rva-syntax](https://github.com/Jjoon0513/rva-syntax))

---

## Installation

### Install from crates.io

```bash
cargo install rva
```

### Install from source

```bash
git clone https://github.com/Jjoon0513/rva.git
cd rva
cargo install --path .
```

Verify the installation:

```bash
rva --version
```

---

## Usage

Assemble an assembly source file:

```bash
rva input.asm
```

This generates:

```
input.bin
```

Specify a custom output file:

```bash
rva input.asm -o output.bin
```

Display all available options:

```bash
rva --help
```

---

## Example

**hello.asm**

```asm
start:
    movi r0, 123
    hlt
```

Assemble it:

```bash
rva hello.asm -o hello.bin
```

The generated binary can then be loaded into
[rustVM](https://github.com/Jjoon0513/rustVM).

---

## Supported Directives

| Directive | Description |
| ---------- | ----------- |
| `db` | Insert one or more bytes |
| `dw` | Insert one or more 16-bit words |

Example:

```asm
db 0x41, 0x42, 0x43
dw 0x1234
```

---

## Project Structure

```
rva/
├── src/
│   ├── lexer.rs
│   ├── encoder.rs
│   ├── opcode_table.rs
│   └── main.rs
├── tests/
│   ├── (IS ANYBODY HERE??)
└── README.md
```

---

## Related Projects

- [rustVM](https://github.com/Jjoon0513/rustVM)

---

## Roadmap

- [ ] Better diagnostics with line/column information
- [ ] Multi-file assembly
- [ ] Symbol visibility
- [ ] Object file generation

---

## License

MIT OR Apache-2.0
