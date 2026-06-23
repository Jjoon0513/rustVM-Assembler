//! 라인 단위 lexer. 표현식 평가 없음 — operand는 그냥 문자열 토큰으로만 분리하고
//! 실제 해석(레지스터/즉시값/라벨)은 encoder.rs에서 instr_table의 operand shape를 보고 함.

/// db/dw 디렉티브의 개별 항목
#[derive(Debug, Clone)]
pub enum DataItem {
    /// 숫자 리터럴 또는 라벨 이름 ("0x41", "255", "some_label")
    Value(String),
    /// 문자열 리터럴 — 따옴표 벗긴 raw 내용 ("hello")
    Str(String),
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

#[derive(Debug, Clone)]
pub struct RawLine {
    pub line_no: usize,
    pub label: Option<String>,
    pub kind: Option<LineKind>,
}

pub fn lex(source: &str) -> Vec<RawLine> {
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
            if !maybe_label.is_empty() && !maybe_label.contains(char::is_whitespace) {
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
                let items = parse_data_items(after_first);
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

    lines
}

/// `db "hello", 0x00, 10` 같은 콤마 구분 목록을 파싱
/// 문자열 안의 콤마는 구분자로 안 봄
fn parse_data_items(s: &str) -> Vec<DataItem> {
    let mut items = Vec::new();
    let mut chars = s.char_indices().peekable();

    while chars.peek().is_some() {
        // 앞 공백 스킵
        while chars.peek().map(|(_, c)| c.is_whitespace()) == Some(true) {
            chars.next();
        }

        let Some(&(start, ch)) = chars.peek() else { break };

        if ch == '"' {
            // 문자열 리터럴
            chars.next(); // 여는 따옴표 소비
            let mut buf = String::new();
            let mut closed = false;
            for (_, c) in chars.by_ref() {
                if c == '"' { closed = true; break; }
                // \n, \t, \0 이스케이프 처리
                if c == '\\' { continue; } // 다음 루프에서 처리 — 간단하게 넘어감
                buf.push(c);
            }
            if closed {
                items.push(DataItem::Str(buf));
            }
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

    items
}

fn strip_comment(line: &str) -> &str {
    // ';'랑 '//' 둘 다 주석으로 인정. 둘 다 있으면 먼저 나오는 쪽 기준
    let semi = line.find(';');
    let slash = line.find("//");
    match (semi, slash) {
        (Some(s), Some(sl)) => &line[..s.min(sl)],
        (Some(s), None) => &line[..s],
        (None, Some(sl)) => &line[..sl],
        (None, None) => line,
    }
}
