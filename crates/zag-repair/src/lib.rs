#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DiagnosticCode {
    MismatchedTypes = 0,
    Unhandled = 1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub span_start: u32,
    pub span_end: u32,
    pub expected: Vec<u8>,
    pub found: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Repair {
    pub span_start: u32,
    pub span_end: u32,
    pub prefix: Vec<u8>,
    pub suffix: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RepairError {
    SpanOutOfRange { span_start: u32, span_end: u32 },
    OverlappingRepairs { earlier: usize, later: usize },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct IntegerType {
    signed: bool,
    width: Option<u32>,
}

fn integer_type(name: &[u8]) -> Option<IntegerType> {
    let (signed, rest) = match name.split_first() {
        Some((b'u', rest)) => (false, rest),
        Some((b'i', rest)) => (true, rest),
        _ => return None,
    };
    if rest == b"size" {
        return Some(IntegerType {
            signed,
            width: None,
        });
    }
    let text = std::str::from_utf8(rest).ok()?;
    let width = text.parse::<u32>().ok()?;
    if !matches!(width, 8 | 16 | 32 | 64 | 128) {
        return None;
    }
    Some(IntegerType {
        signed,
        width: Some(width),
    })
}

fn widens(found: IntegerType, expected: IntegerType) -> bool {
    if found.signed != expected.signed {
        return false;
    }
    match (found.width, expected.width) {
        (Some(from), Some(to)) => from < to,
        _ => false,
    }
}

fn repair_for(diagnostic: &Diagnostic) -> Option<Repair> {
    if diagnostic.code != DiagnosticCode::MismatchedTypes {
        return None;
    }
    if diagnostic.span_end < diagnostic.span_start {
        return None;
    }
    let expected = integer_type(&diagnostic.expected)?;
    let found = integer_type(&diagnostic.found)?;
    if expected == found {
        return None;
    }
    let mut prefix = diagnostic.expected.clone();
    let suffix: &[u8] = if widens(found, expected) {
        prefix.extend_from_slice(b"::from(");
        b")"
    } else {
        prefix.extend_from_slice(b"::try_from(");
        b").unwrap()"
    };
    Some(Repair {
        span_start: diagnostic.span_start,
        span_end: diagnostic.span_end,
        prefix,
        suffix: suffix.to_vec(),
    })
}

pub fn plan_repairs(diagnostics: &[Diagnostic]) -> Vec<Repair> {
    diagnostics.iter().filter_map(repair_for).collect()
}

pub fn apply_repairs(source: &[u8], repairs: &[Repair]) -> Result<Vec<u8>, RepairError> {
    let mut order: Vec<usize> = (0..repairs.len()).collect();
    order.sort_by_key(|index| (repairs[*index].span_start, repairs[*index].span_end));
    for repair in repairs {
        if repair.span_end < repair.span_start || repair.span_end as usize > source.len() {
            return Err(RepairError::SpanOutOfRange {
                span_start: repair.span_start,
                span_end: repair.span_end,
            });
        }
    }
    for window in order.windows(2) {
        let earlier = &repairs[window[0]];
        let later = &repairs[window[1]];
        if later.span_start < earlier.span_end {
            return Err(RepairError::OverlappingRepairs {
                earlier: window[0],
                later: window[1],
            });
        }
    }
    let mut out = Vec::with_capacity(source.len());
    let mut cursor = 0usize;
    for index in order {
        let repair = &repairs[index];
        out.extend_from_slice(&source[cursor..repair.span_start as usize]);
        out.extend_from_slice(&repair.prefix);
        out.extend_from_slice(&source[repair.span_start as usize..repair.span_end as usize]);
        out.extend_from_slice(&repair.suffix);
        cursor = repair.span_end as usize;
    }
    out.extend_from_slice(&source[cursor..]);
    Ok(out)
}
