//! 라인 단위 lexer. 표현식 평가 없음 — operand는 그냥 문자열 토큰으로만 분리하고
//! 실제 해석(레지스터/즉시값/라벨)은 encoder.rs에서 instr_table의 operand shape를 보고 함.

#[derive(Debug, Clone)]
pub struct RawLine {
    pub line_no: usize,
    pub label: Option<String>,
    pub mnemonic: Option<String>,
    pub operands: Vec<String>,
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
            // 라벨만 있는 줄 (예: "loop:" 단독)
            lines.push(RawLine { line_no, label, mnemonic: None, operands: vec![] });
            continue;
        }

        // 공백/콤마 아무거나로 구분 — "movi r0, 10"이랑 "movi r0 10" 둘 다 허용
        let mut tokens = rest
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|s| !s.is_empty());

        let mnemonic = tokens.next().map(|s| s.to_lowercase());
        let operands = tokens.map(|s| s.to_string()).collect();

        lines.push(RawLine { line_no, label, mnemonic, operands });
    }

    lines
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
