//! 라인 단위 lexer. 표현식 평가 없음 — operand는 그냥 문자열 토큰으로만 분리하고
//! 실제 해석(레지스터/즉시값/라벨)은 encoder.rs에서 instr_table의 operand shape를 보고 함.

/// db/dw 디렉티브의 개별 항목
#[derive(Debug, Clone)]
pub enum DataItem {
    /// 숫자 리터럴 또는 라벨 이름 ("0x41", "255", "some_label")
    Value(String),
    /// 문자열 리터럴 — 이스케이프가 이미 해석된 **최종 바이트열** (UTF-8, `\xNN` 포함).
    /// 길이 계산(pass 1)과 출력(pass 2)이 같은 바이트열을 쓰도록 렉서에서 확정한다.
    Str(Vec<u8>),
}

#[derive(Debug, Clone)]
pub enum LineKind {
    /// 일반 명령어
    Instr { mnemonic: String, operands: Vec<String> },
    /// db: 바이트 단위 데이터
    Db(Vec<DataItem>),
    /// dw: 16비트 단위 데이터 (low, high 순서로 2바이트씩)
    Dw(Vec<DataItem>),
}

#[derive(Debug)]
pub struct LexError {
    pub line_no: usize,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct RawLine {
    pub line_no: usize,
    pub label: Option<String>,
    pub kind: Option<LineKind>,
}

pub fn lex(source: &str) -> Result<Vec<RawLine>, LexError> {
    let mut lines = Vec::new();

    for (i, raw_line) in source.lines().enumerate() {
        let line_no = i + 1;
        let code = strip_comment(raw_line).trim();
        if code.is_empty() {
            continue;
        }

        let mut rest = code;
        let mut label = None;

        // "label: mnemonic operands" 형태에서 콜론 앞부분을 라벨로 취급
        if let Some(colon_idx) = rest.find(':') {
            let (maybe_label, after) = rest.split_at(colon_idx);
            let maybe_label = maybe_label.trim();
            if !maybe_label.is_empty()
                && !maybe_label.contains(char::is_whitespace)
                && !maybe_label.contains('"')
            {
                label = Some(maybe_label.to_string());
                rest = after[1..].trim();
            }
        }

        if rest.is_empty() {
            lines.push(RawLine { line_no, label, kind: None });
            continue;
        }

        // 첫 토큰만 떼서 디렉티브인지 확인
        let first_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let first = rest[..first_end].to_lowercase();
        let after_first = rest[first_end..].trim();

        let kind = match first.as_str() {
            "db" | "dw" => {
                let items = parse_data_items(after_first)
                    .map_err(|message| LexError { line_no, message })?;
                if first == "db" { LineKind::Db(items) } else { LineKind::Dw(items) }
            }
            _ => {
                // 일반 명령어 — 공백/콤마로 구분 (문자열 없으니까 단순 split 가능)
                let mut tokens = rest
                    .split(|c: char| c == ',' || c.is_whitespace())
                    .filter(|s| !s.is_empty());
                let mnemonic = tokens.next().unwrap().to_lowercase();
                let operands = tokens.map(|s| s.to_string()).collect();
                LineKind::Instr { mnemonic, operands }
            }
        };

        lines.push(RawLine { line_no, label, kind: Some(kind) });
    }

    Ok(lines)
}

/// `db "hello", 0x00, 10` 같은 콤마 구분 목록을 파싱
/// 문자열 안의 콤마는 구분자로 안 보고, 이스케이프(`\n \t \r \0 \\ \" \xNN`)는 여기서 해석한다.
fn parse_data_items(s: &str) -> Result<Vec<DataItem>, String> {
    let mut items = Vec::new();
    let mut chars = s.char_indices().peekable();

    while chars.peek().is_some() {
        // 앞 공백 스킵
        while chars.peek().map(|(_, c)| c.is_whitespace()) == Some(true) {
            chars.next();
        }

        let Some(&(start, ch)) = chars.peek() else { break };

        if ch == '"' {
            chars.next(); // 여는 따옴표 소비
            let mut buf: Vec<u8> = Vec::new();
            let mut closed = false;
            while let Some((_, c)) = chars.next() {
                match c {
                    '"' => {
                        closed = true;
                        break;
                    }
                    '\\' => {
                        let Some((_, esc)) = chars.next() else { break };
                        match esc {
                            'n' => buf.push(b'\n'),
                            't' => buf.push(b'\t'),
                            'r' => buf.push(b'\r'),
                            '0' => buf.push(0),
                            '\\' => buf.push(b'\\'),
                            '"' => buf.push(b'"'),
                            'x' => {
                                let hi = chars.next().and_then(|(_, c)| c.to_digit(16));
                                let lo = chars.next().and_then(|(_, c)| c.to_digit(16));
                                match (hi, lo) {
                                    (Some(h), Some(l)) => buf.push((h * 16 + l) as u8),
                                    _ => return Err("invalid \\x escape (expected two hex digits)".to_string()),
                                }
                            }
                            other => {
                                return Err(format!("unknown escape sequence '\\{other}'"));
                            }
                        }
                    }
                    other => {
                        let mut tmp = [0u8; 4];
                        buf.extend_from_slice(other.encode_utf8(&mut tmp).as_bytes());
                    }
                }
            }
            if !closed {
                return Err("unterminated string literal".to_string());
            }
            items.push(DataItem::Str(buf));
        } else if ch != ',' {
            // 숫자 또는 라벨
            let mut end = start;
            for (i, c) in chars.by_ref() {
                if c == ',' || c.is_whitespace() { break; }
                end = i + c.len_utf8();
            }
            let token = s[start..end].trim().to_string();
            if !token.is_empty() {
                items.push(DataItem::Value(token));
            }
            continue;
        }

        // 콤마 소비
        if chars.peek().map(|(_, c)| *c) == Some(',') {
            chars.next();
        }
    }

    Ok(items)
}

/// `;` 또는 `//` 이후를 주석으로 잘라낸다. 단, 문자열 리터럴(`"..."`) 안은 건드리지 않는다.
fn strip_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut in_str = false;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if in_str {
            match b {
                b'\\' => i += 1, // 이스케이프된 다음 바이트는 건너뜀 (\" 가 문자열을 닫지 않도록)
                b'"' => in_str = false,
                _ => {}
            }
        } else {
            match b {
                b'"' => in_str = true,
                b';' => return &line[..i],
                b'/' if bytes.get(i + 1) == Some(&b'/') => return &line[..i],
                _ => {}
            }
        }
        i += 1;
    }
    line
}
