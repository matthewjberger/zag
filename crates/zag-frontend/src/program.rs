//! The two tool outputs, parsed into something the table builder can walk.
//! Both are line oriented and every line is `<kind> <subject> key=value...`,
//! so one reader serves both.

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Container {
    pub name: String,
    pub kind: String,
    pub members: Vec<Member>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Member {
    pub name: String,
    pub declared: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Function {
    pub name: String,
    pub owner: String,
    pub returns: String,
    /// One-based line in the file that declares it. Zero where the reader had
    /// none to give, which is what a hand-built table leaves behind.
    pub line: u32,
    pub parameters: Vec<Parameter>,
    pub calls: Vec<Call>,
    pub locals: Vec<(String, String)>,
    pub initialisers: Vec<Initialiser>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parameter {
    pub name: String,
    pub declared: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Call {
    pub callee: String,
    pub arguments: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Initialiser {
    pub node: u32,
    pub parent: Option<u32>,
    pub field: String,
    pub line: u32,
    pub value: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Layout {
    pub size: u32,
    pub alignment: u32,
    pub is_extern: bool,
    pub offsets: Vec<(String, u32)>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Program {
    pub containers: Vec<Container>,
    pub functions: Vec<Function>,
    pub layouts: Vec<(String, Layout)>,
}

fn value<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let start = line.find(&format!(" {key}="))? + key.len() + 2;
    let rest = &line[start..];
    if key == "value" || key == "type" || key == "returns" {
        return Some(rest);
    }
    Some(rest.split_whitespace().next().unwrap_or(rest))
}

fn number(line: &str, key: &str) -> u32 {
    value(line, key)
        .and_then(|text| text.parse().ok())
        .unwrap_or_default()
}

fn split_once_at(subject: &str, separator: char) -> (&str, &str) {
    subject.split_once(separator).unwrap_or((subject, ""))
}

fn function_mut<'a>(program: &'a mut Program, name: &str) -> Option<&'a mut Function> {
    program
        .functions
        .iter_mut()
        .find(|function| function.name == name)
}

fn container_mut<'a>(program: &'a mut Program, name: &str) -> Option<&'a mut Container> {
    program
        .containers
        .iter_mut()
        .find(|container| container.name == name)
}

pub fn parse_extraction(text: &str, program: &mut Program) {
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let (Some(kind), Some(subject)) = (parts.next(), parts.next()) else {
            continue;
        };
        match kind {
            "container" => program.containers.push(Container {
                name: subject.to_string(),
                kind: value(line, "kind").unwrap_or("struct").to_string(),
                members: Vec::new(),
            }),
            "member" => {
                let (owner, name) = split_once_at(subject, '.');
                let declared = value(line, "type").unwrap_or("-").to_string();
                if let Some(container) = container_mut(program, owner) {
                    container.members.push(Member {
                        name: name.to_string(),
                        declared,
                    });
                }
            }
            "function" => program.functions.push(Function {
                name: subject.to_string(),
                owner: value(line, "owner").unwrap_or("-").to_string(),
                returns: value(line, "returns").unwrap_or("-").to_string(),
                line: number(line, "line"),
                ..Function::default()
            }),
            "parameter" => {
                let (owner, _) = split_once_at(subject, '.');
                let name = value(line, "name").unwrap_or("-").to_string();
                let declared = value(line, "type").unwrap_or("-").to_string();
                if let Some(function) = function_mut(program, owner) {
                    function.parameters.push(Parameter { name, declared });
                }
            }
            "call" => {
                let callee = value(line, "callee").unwrap_or("-").to_string();
                if let Some(function) = function_mut(program, subject) {
                    function.calls.push(Call {
                        callee,
                        arguments: Vec::new(),
                    });
                }
            }
            "argument" => {
                let mut pieces = subject.split('|');
                let owner = pieces.next().unwrap_or("");
                let callee = pieces.next().unwrap_or("");
                let text = value(line, "text").unwrap_or("").to_string();
                if let Some(function) = function_mut(program, owner)
                    && let Some(call) = function
                        .calls
                        .iter_mut()
                        .find(|call| call.callee == callee && call.arguments.len() < 32)
                {
                    call.arguments.push(text);
                }
            }
            "local" => {
                let (owner, name) = split_once_at(subject, '.');
                let initialiser = value(line, "value").unwrap_or("").to_string();
                if let Some(function) = function_mut(program, owner) {
                    function.locals.push((name.to_string(), initialiser));
                }
            }
            "initialiser" => {
                let node = number(line, "node");
                let parent = value(line, "parent")
                    .filter(|text| *text != "-")
                    .and_then(|text| text.parse().ok());
                let field = value(line, "field").unwrap_or("-").to_string();
                let entry = value(line, "value").unwrap_or("").to_string();
                if let Some(function) = function_mut(program, subject) {
                    function.initialisers.push(Initialiser {
                        node,
                        parent,
                        field,
                        line: number(line, "line"),
                        value: entry,
                    });
                }
            }
            _ => {}
        }
    }
}

pub fn parse_reflection(text: &str, program: &mut Program) {
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let (Some(kind), Some(subject)) = (parts.next(), parts.next()) else {
            continue;
        };
        match kind {
            "struct" => program.layouts.push((
                subject.to_string(),
                Layout {
                    size: number(line, "size"),
                    alignment: number(line, "align"),
                    is_extern: value(line, "layout") == Some("extern"),
                    offsets: Vec::new(),
                },
            )),
            "field" => {
                let (owner, name) = split_once_at(subject, '.');
                let offset = number(line, "offset");
                if let Some(entry) = program
                    .layouts
                    .iter_mut()
                    .find(|(layout, _)| layout == owner)
                {
                    entry.1.offsets.push((name.to_string(), offset));
                }
            }
            _ => {}
        }
    }
}

pub fn parse(extraction: &str, reflection: &str) -> Program {
    let mut program = Program::default();
    parse_extraction(extraction, &mut program);
    parse_reflection(reflection, &mut program);
    program
}
